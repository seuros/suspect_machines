//! Helpers shared by the crate's test modules: a deterministic generator, an
//! independent phi reference and a naive detector model.
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use core::time::Duration;
use std::vec::Vec;

use crate::{Beat, Config, Detector, Distribution, Refusal, Roster, Suspicion, Threshold};

pub const SECOND: u64 = 1_000_000_000;
pub const MILLI: u64 = 1_000_000;

pub fn at(nanoseconds: u64) -> Duration {
    Duration::from_nanos(nanoseconds)
}

pub fn normal(floor_ns: u64, min_samples: usize) -> Config {
    let distribution = Distribution::Normal {
        min_std_dev: at(floor_ns),
    };
    Config::new(distribution, min_samples).unwrap_or_else(|error| panic!("{error:?}"))
}

pub fn exponential(min_samples: usize) -> Config {
    Config::new(Distribution::Exponential, min_samples).unwrap_or_else(|error| panic!("{error:?}"))
}

pub fn detector<const W: usize>(config: Config) -> Detector<W> {
    Detector::new(config).unwrap_or_else(|error| panic!("{error:?}"))
}

pub fn roster<const P: usize, const W: usize>(config: Config) -> Roster<P, W> {
    Roster::new(config).unwrap_or_else(|error| panic!("{error:?}"))
}

pub fn threshold(phi: f64) -> Threshold {
    Threshold::new(phi).unwrap_or_else(|| panic!("bad threshold {phi}"))
}

pub fn measured(suspicion: Result<Suspicion, Refusal>) -> f64 {
    let Ok(Suspicion::Measured(phi)) = suspicion else {
        panic!("expected a phi, got {suspicion:?}");
    };
    phi.value()
}

pub fn close(actual: f64, expected: f64, tolerance: f64) -> bool {
    let error = (actual - expected).abs();
    error <= tolerance || error <= tolerance * expected.abs()
}

/// Deterministic xorshift64* generator for the randomized tests.
pub struct Rng(u64);

impl Rng {
    pub const fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub const fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub const fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// `-log10(Q(z))` by Simpson quadrature of the Gaussian tail with `std`
/// math: `Q(a) = pdf(a) * integral over u >= 0 of exp(-a u - u^2 / 2)`.
/// Shares no code and no method with `math::normal_phi`.
pub fn reference_normal_phi(z: f64) -> f64 {
    const STEPS: u32 = 8_000;
    let a = z.abs();
    let upper = if a > 1.0 { 45.0 / a } else { 45.0 };
    let width = upper / f64::from(STEPS);
    let integrand = |u: f64| (-a * u - 0.5 * u * u).exp();
    let mut total = integrand(0.0) + integrand(upper);
    for step in 1..STEPS {
        let weight = if step % 2 == 1 { 4.0 } else { 2.0 };
        total += weight * integrand(f64::from(step) * width);
    }
    let integral = total * width / 3.0;
    let ln_tail = -0.5 * a * a - 0.5 * (2.0 * std::f64::consts::PI).ln() + integral.ln();
    let ln_survival = if z >= 0.0 {
        ln_tail
    } else {
        (-ln_tail.exp()).ln_1p()
    };
    (-ln_survival / std::f64::consts::LN_10).max(0.0)
}

/// Naive reference: every interval kept in a growable list, statistics
/// recomputed in arrival order with `std` math, phi from quadrature.
#[derive(Clone, Default)]
pub struct Model {
    pub intervals: Vec<u64>,
    pub last: Option<u64>,
}

/// What the naive model expects a suspicion read to produce.
#[derive(Debug)]
pub enum Expected {
    Refused,
    Undefined,
    Phi(f64),
}

impl Model {
    pub fn observe(&mut self, now: u64, window: usize) -> Result<Beat, Refusal> {
        let Some(last) = self.last else {
            self.last = Some(now);
            return Ok(Beat::First);
        };
        if now < last {
            return Err(Refusal::TimeRegression);
        }
        if now == last {
            return Err(Refusal::Duplicate);
        }
        self.intervals.push(now - last);
        if self.intervals.len() > window {
            self.intervals.remove(0);
        }
        self.last = Some(now);
        Ok(Beat::Recorded {
            interval: at(now - last),
            samples: self.intervals.len(),
        })
    }

    pub fn mean(&self) -> f64 {
        self.intervals.iter().map(|&x| x as f64).sum::<f64>() / self.intervals.len() as f64
    }

    pub fn std_dev(&self) -> f64 {
        let mean = self.mean();
        let squares: f64 = self
            .intervals
            .iter()
            .map(|&x| (x as f64 - mean).powi(2))
            .sum();
        (squares / self.intervals.len() as f64).sqrt()
    }

    pub fn phi(&self, config: &Config, now: u64) -> Expected {
        let Some(last) = self.last else {
            return Expected::Undefined;
        };
        if now < last {
            return Expected::Refused;
        }
        if self.intervals.len() < config.min_samples() {
            return Expected::Undefined;
        }
        let silence = (now - last) as f64;
        let pause = config.acceptable_pause().as_nanos() as f64;
        Expected::Phi(match config.distribution() {
            Distribution::Normal { min_std_dev } => {
                let deviation = self.std_dev().max(min_std_dev.as_nanos() as f64);
                reference_normal_phi((silence - self.mean() - pause) / deviation)
            }
            Distribution::Exponential => {
                (((silence - pause) / self.mean()) * std::f64::consts::LOG10_E).max(0.0)
            }
        })
    }
}

pub fn random_config(rng: &mut Rng, window: usize) -> Config {
    let min_samples = 1 + rng.below(window as u64) as usize;
    let config = if rng.below(2) == 0 {
        normal(1_000 + rng.below(50 * MILLI), min_samples)
    } else {
        exponential(min_samples)
    };
    config
        .with_acceptable_pause(at(rng.below(3) * rng.below(200 * MILLI)))
        .unwrap_or_else(|error| panic!("{error:?}"))
}

/// A step that is usually a plausible heartbeat gap, sometimes zero.
pub fn random_gap(rng: &mut Rng) -> u64 {
    match rng.below(10) {
        0 => 0,
        1 => 1 + rng.below(30 * SECOND),
        _ => 900 * MILLI + rng.below(200 * MILLI),
    }
}
