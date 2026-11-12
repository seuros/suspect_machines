#![no_std]
#![doc = include_str!("../README.md")]

#[cfg(test)]
extern crate std;

mod detector;
mod math;
mod roster;
#[cfg(test)]
mod test_support;
mod window;

use core::time::Duration;

pub use detector::Detector;
pub use roster::{Roster, Scan, Suspect};

/// The distribution the detector assumes heartbeat intervals follow. This is
/// an assumption the caller makes about its network, not something the
/// detector verifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Distribution {
    /// Intervals are normally distributed around the window mean, as in
    /// Hayashibara et al. The window's population standard deviation is
    /// floored at `min_std_dev`, so a perfectly regular sender does not make
    /// the first late nanosecond infinitely suspicious.
    Normal {
        /// Non-zero floor for the standard deviation.
        min_std_dev: Duration,
    },
    /// Intervals are exponentially distributed with the window mean. Only the
    /// mean is used: `phi = silence / (mean * ln 10)`.
    Exponential,
}

/// Why a configuration was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigError {
    /// A warm-up of zero intervals would report phi from no data.
    ZeroWarmUp,
    /// The normal model needs a non-zero deviation floor.
    ZeroDeviationFloor,
    /// A duration does not fit in `u64` nanoseconds.
    DurationOverflow,
    /// The interval window has no slots.
    ZeroWindow,
    /// The warm-up asks for more intervals than the window can hold.
    WarmUpExceedsWindow,
}

/// Validated detector policy: distribution, warm-up and acceptable pause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    distribution: Distribution,
    min_samples: usize,
    pause_ns: u64,
}

impl Config {
    /// A policy that reports phi only once `min_samples` intervals are in the
    /// window, under the given distribution assumption.
    ///
    /// # Errors
    ///
    /// [`ConfigError::ZeroWarmUp`] for zero `min_samples`,
    /// [`ConfigError::ZeroDeviationFloor`] or
    /// [`ConfigError::DurationOverflow`] for an unusable normal floor.
    pub const fn new(distribution: Distribution, min_samples: usize) -> Result<Self, ConfigError> {
        if min_samples == 0 {
            return Err(ConfigError::ZeroWarmUp);
        }
        if let Distribution::Normal { min_std_dev } = distribution {
            match nanos(min_std_dev) {
                None => return Err(ConfigError::DurationOverflow),
                Some(0) => return Err(ConfigError::ZeroDeviationFloor),
                Some(_) => {}
            }
        }
        Ok(Self {
            distribution,
            min_samples,
            pause_ns: 0,
        })
    }

    /// Tolerates `pause` of extra silence before suspicion starts to accrue:
    /// the pause is added to the expected interval (normal) or subtracted
    /// from the silence (exponential).
    ///
    /// # Errors
    ///
    /// [`ConfigError::DurationOverflow`] when `pause` exceeds `u64`
    /// nanoseconds.
    pub const fn with_acceptable_pause(mut self, pause: Duration) -> Result<Self, ConfigError> {
        match nanos(pause) {
            Some(pause_ns) => {
                self.pause_ns = pause_ns;
                Ok(self)
            }
            None => Err(ConfigError::DurationOverflow),
        }
    }

    /// The assumed interval distribution.
    #[must_use]
    pub const fn distribution(&self) -> Distribution {
        self.distribution
    }

    /// Intervals required before phi is reported.
    #[must_use]
    pub const fn min_samples(&self) -> usize {
        self.min_samples
    }

    /// Extra silence tolerated before suspicion accrues.
    #[must_use]
    pub const fn acceptable_pause(&self) -> Duration {
        Duration::from_nanos(self.pause_ns)
    }

    /// Checks the policy against a window of `window` intervals.
    const fn fit(self, window: usize) -> Result<Self, ConfigError> {
        if window == 0 {
            return Err(ConfigError::ZeroWindow);
        }
        if self.min_samples > window {
            return Err(ConfigError::WarmUpExceedsWindow);
        }
        Ok(self)
    }

    /// Phi for `silence_ns` without a heartbeat, given the window statistics.
    fn phi(&self, statistics: &Statistics, silence_ns: u64) -> Phi {
        let silence = window::to_f64(silence_ns);
        let pause = window::to_f64(self.pause_ns);
        Phi(match self.distribution {
            Distribution::Normal { min_std_dev } => {
                let floor = window::to_f64(nanos(min_std_dev).unwrap_or(u64::MAX));
                let deviation = if statistics.std_dev_ns > floor {
                    statistics.std_dev_ns
                } else {
                    floor
                };
                math::normal_phi((silence - statistics.mean_ns - pause) / deviation)
            }
            Distribution::Exponential => {
                math::exponential_phi((silence - pause) / statistics.mean_ns)
            }
        })
    }
}

/// Why a call was refused. A refused call changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refusal {
    /// `now` is earlier than the last recorded heartbeat.
    TimeRegression,
    /// `now` equals the last recorded heartbeat: a zero interval carries no
    /// timing information and is not recorded.
    Duplicate,
    /// `now` does not fit in `u64` nanoseconds.
    Overflow,
    /// The heartbeat carries an incarnation older than the tracked one.
    Stale,
    /// Every roster slot tracks another peer.
    Full,
    /// The roster does not track this peer.
    Untracked,
}

/// What an accepted heartbeat did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Beat {
    /// First heartbeat: a reference point, no interval yet.
    First,
    /// An interval entered the window, evicting the oldest one when full.
    Recorded {
        /// Time since the previous heartbeat.
        interval: Duration,
        /// Intervals now in the window.
        samples: usize,
    },
    /// A newer incarnation replaced the tracked one; its statistics start
    /// over from this heartbeat.
    Restarted,
}

/// A suspicion level: `-log10` of the probability, under the configured
/// distribution, that a heartbeat would still arrive after this much silence.
///
/// Phi 1 means roughly a 10 % chance the peer is merely late, phi 3 roughly
/// 0.1 %. It is a statement about arrival statistics. It is not a verdict
/// that the peer is dead and it revokes nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd)]
pub struct Phi(f64);

impl Phi {
    /// The level as a finite, non-negative `f64`.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }

    /// The level in thousandths, rounded down and saturating at `u64::MAX`.
    #[must_use]
    pub const fn millis(self) -> u64 {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "phi is non-negative and float-to-int casts saturate"
        )]
        let millis = (self.0 * 1000.0) as u64;
        millis
    }

    /// Whether the level has reached `threshold`.
    #[must_use]
    pub fn reaches(self, threshold: Threshold) -> bool {
        self.0 >= threshold.0
    }
}

/// A finite, positive phi level the caller treats as "worth acting on".
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Threshold(f64);

impl Threshold {
    /// `None` unless `phi` is finite and greater than zero.
    #[must_use]
    pub const fn new(phi: f64) -> Option<Self> {
        if phi.is_finite() && phi > 0.0 {
            Some(Self(phi))
        } else {
            None
        }
    }

    /// The threshold level.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }
}

/// What the detector can say about a peer at a given instant.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Suspicion {
    /// No heartbeat has ever been recorded.
    NoHeartbeat,
    /// Fewer intervals than the configured warm-up: no phi is computed. The
    /// silence is reported so the caller can apply its own bootstrap timeout.
    WarmingUp {
        /// Intervals in the window.
        samples: usize,
        /// Intervals the warm-up requires.
        required: usize,
        /// Time since the last heartbeat.
        silence: Duration,
    },
    /// The suspicion level under the configured distribution.
    Measured(Phi),
}

/// Statistics of the intervals currently in the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Statistics {
    /// Intervals in the window.
    pub samples: usize,
    /// Arithmetic mean, in nanoseconds.
    pub mean_ns: f64,
    /// Population standard deviation, in nanoseconds, before any floor.
    pub std_dev_ns: f64,
    /// Shortest interval.
    pub min: Duration,
    /// Longest interval.
    pub max: Duration,
}

/// `duration` in nanoseconds when it fits `u64` (about 584 years).
const fn nanos(duration: Duration) -> Option<u64> {
    let wide = duration.as_nanos();
    if wide > u64::MAX as u128 {
        None
    } else {
        #[allow(clippy::cast_possible_truncation, reason = "range checked above")]
        Some(wide as u64)
    }
}

#[cfg(test)]
mod tests;
