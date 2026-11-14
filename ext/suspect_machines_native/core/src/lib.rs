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
pub use roster::{Forgetting, Roster, Scan, Suspect};

/// The distribution the detector assumes heartbeat intervals follow. This is
/// an assumption the caller makes about its network, not something the
/// detector verifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distribution {
    /// Intervals are normally distributed around the window mean, as in
    /// Hayashibara et al., with the deviation floor of [`Normal`].
    Normal(Normal),
    /// Intervals are exponentially distributed with the window mean. Only the
    /// mean is used: `phi = silence / (mean * ln 10)`.
    Exponential,
}

/// The normal model's deviation floor: the window's population standard
/// deviation is floored at `min_std_dev`, so a perfectly regular sender does
/// not make the first late nanosecond infinitely suspicious.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Normal {
    floor_ns: u64,
}

impl Normal {
    /// A normal model floored at `min_std_dev`.
    ///
    /// # Errors
    ///
    /// [`ConfigError::ZeroDeviationFloor`] for a zero floor and
    /// [`ConfigError::DurationOverflow`] past `u64` nanoseconds.
    pub const fn new(min_std_dev: Duration) -> Result<Self, ConfigError> {
        match nanos(min_std_dev) {
            None => Err(ConfigError::DurationOverflow),
            Some(0) => Err(ConfigError::ZeroDeviationFloor),
            Some(floor_ns) => Ok(Self { floor_ns }),
        }
    }

    /// The non-zero floor for the standard deviation.
    #[must_use]
    pub const fn min_std_dev(self) -> Duration {
        Duration::from_nanos(self.floor_ns)
    }
}

/// Why a configuration was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// [`ConfigError::ZeroWarmUp`] for zero `min_samples`.
    pub const fn new(distribution: Distribution, min_samples: usize) -> Result<Self, ConfigError> {
        if min_samples == 0 {
            return Err(ConfigError::ZeroWarmUp);
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
            Distribution::Normal(normal) => {
                let floor = window::to_f64(normal.floor_ns);
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
pub enum Refusal {
    /// `now` is earlier than the last recorded heartbeat.
    TimeRegression,
    /// `now` equals the last recorded heartbeat: a zero interval carries no
    /// timing information and is not recorded.
    SameInstant,
    /// `now` does not fit in `u64` nanoseconds.
    Overflow,
    /// The heartbeat carries an incarnation older than the tracked one.
    Stale,
    /// Every roster slot tracks another peer.
    Full,
    /// The roster does not track this peer.
    Untracked,
}

impl core::fmt::Display for Refusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::TimeRegression => "the instant precedes the last recorded heartbeat",
            Self::SameInstant => "the instant equals the last recorded heartbeat",
            Self::Overflow => "the instant does not fit in 64-bit nanoseconds",
            Self::Stale => "the incarnation is older than the tracked one",
            Self::Full => "every roster slot tracks another peer",
            Self::Untracked => "the roster does not track this peer",
        })
    }
}

impl core::error::Error for Refusal {}

/// What an accepted heartbeat did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl Beat {
    /// The [`Beat::Recorded`] a window of `W` intervals can answer: `None`
    /// for a zero interval (refused as [`Refusal::SameInstant`]) or a sample
    /// count outside `1..=W`.
    #[must_use]
    pub const fn recorded<const W: usize>(interval: Duration, samples: usize) -> Option<Self> {
        if interval.is_zero() || samples == 0 || samples > W {
            return None;
        }
        Some(Self::Recorded { interval, samples })
    }
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
    /// `None` unless `value` is finite and not negative. A negative zero is
    /// stored as zero.
    #[must_use]
    pub const fn new(value: f64) -> Option<Self> {
        if value.is_finite() && value >= 0.0 {
            Some(Self(if value == 0.0 { 0.0 } else { value }))
        } else {
            None
        }
    }

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

impl Suspicion {
    /// The [`Suspicion::WarmingUp`] a window of `W` intervals can answer:
    /// `None` unless `samples < required <= W`, since a warm-up is at least
    /// one interval and never longer than the window.
    #[must_use]
    pub const fn warming_up<const W: usize>(
        samples: usize,
        required: usize,
        silence: Duration,
    ) -> Option<Self> {
        if samples >= required || required > W {
            return None;
        }
        Some(Self::WarmingUp {
            samples,
            required,
            silence,
        })
    }
}

/// Statistics of the intervals currently in the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Statistics {
    samples: usize,
    mean_ns: f64,
    std_dev_ns: f64,
    min: Duration,
    max: Duration,
}

impl Statistics {
    /// Statistics a window of `W` intervals can report: `None` unless
    /// `samples` is in `1..=W`, `1 ns <= min <= max` (a zero interval is
    /// never stored, and one interval is its own min and max), the mean lies
    /// within `min..=max`, and the deviation is a finite, positively signed
    /// number no more than half of `max - min` (Popoviciu's bound), all
    /// compared as the `f64` nanoseconds the window computes with.
    #[must_use]
    pub const fn new<const W: usize>(
        samples: usize,
        mean_ns: f64,
        std_dev_ns: f64,
        min: Duration,
        max: Duration,
    ) -> Option<Self> {
        let counted = samples > 0 && samples <= W;
        let ordered = !min.is_zero()
            && min.as_nanos() <= max.as_nanos()
            && (samples > 1 || min.as_nanos() == max.as_nanos());
        let (low, high) = (duration_to_f64(min), duration_to_f64(max));
        let centred = mean_ns >= low && mean_ns <= high;
        let spread = std_dev_ns.is_sign_positive() && std_dev_ns <= half_range(low, high);
        if counted && ordered && centred && spread {
            Some(Self {
                samples,
                mean_ns,
                std_dev_ns,
                min,
                max,
            })
        } else {
            None
        }
    }

    /// Intervals in the window.
    #[must_use]
    pub const fn samples(&self) -> usize {
        self.samples
    }

    /// Arithmetic mean, in nanoseconds.
    #[must_use]
    pub const fn mean_ns(&self) -> f64 {
        self.mean_ns
    }

    /// Population standard deviation, in nanoseconds, before any floor.
    #[must_use]
    pub const fn std_dev_ns(&self) -> f64 {
        self.std_dev_ns
    }

    /// Shortest interval.
    #[must_use]
    pub const fn min(&self) -> Duration {
        self.min
    }

    /// Longest interval.
    #[must_use]
    pub const fn max(&self) -> Duration {
        self.max
    }
}

/// A duration's nanoseconds as the nearest `f64`.
#[allow(
    clippy::cast_precision_loss,
    reason = "nanoseconds round to the nearest f64, as the window computes"
)]
pub(crate) const fn duration_to_f64(duration: Duration) -> f64 {
    duration.as_nanos() as f64
}

/// Half the spread between two interval lengths: the largest population
/// standard deviation intervals within them can have (Popoviciu).
pub(crate) const fn half_range(low: f64, high: f64) -> f64 {
    (high - low) / 2.0
}

/// `now` as `u64` nanoseconds, or [`Refusal::Overflow`].
const fn instant(now: Duration) -> Result<u64, Refusal> {
    match nanos(now) {
        Some(now) => Ok(now),
        None => Err(Refusal::Overflow),
    }
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
