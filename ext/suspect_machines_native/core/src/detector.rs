//! Phi-accrual detector for a single peer.

use core::time::Duration;

use crate::{Beat, Config, ConfigError, Refusal, Statistics, Suspicion, nanos, window::Window};

/// Heartbeat statistics and suspicion for one peer over the last `W`
/// intervals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detector<const W: usize> {
    config: Config,
    window: Window<W>,
}

impl<const W: usize> Detector<W> {
    /// Intervals the window holds.
    pub const WINDOW: usize = W;

    /// An empty detector.
    ///
    /// # Errors
    ///
    /// [`ConfigError::ZeroWindow`] when `W` is zero and
    /// [`ConfigError::WarmUpExceedsWindow`] when the warm-up could never
    /// finish.
    pub const fn new(config: Config) -> Result<Self, ConfigError> {
        match config.fit(W) {
            Ok(config) => Ok(Self {
                config,
                window: Window::new(),
            }),
            Err(error) => Err(error),
        }
    }

    /// Records a heartbeat observed at `now`, a caller-supplied monotonic
    /// time. A full window drops its oldest interval.
    ///
    /// # Errors
    ///
    /// [`Refusal::Overflow`], [`Refusal::TimeRegression`] or
    /// [`Refusal::SameInstant`]; the detector is unchanged.
    pub fn heartbeat(&mut self, now: Duration) -> Result<Beat, Refusal> {
        self.window.observe(nanos(now).ok_or(Refusal::Overflow)?)
    }

    /// The suspicion at `now`. Reading never changes the detector.
    ///
    /// # Errors
    ///
    /// [`Refusal::Overflow`], or [`Refusal::TimeRegression`] when `now`
    /// precedes the last heartbeat.
    pub fn suspicion(&self, now: Duration) -> Result<Suspicion, Refusal> {
        self.window
            .suspicion(&self.config, nanos(now).ok_or(Refusal::Overflow)?)
    }

    /// Statistics of the stored intervals; `None` while there are none.
    #[must_use]
    pub fn statistics(&self) -> Option<Statistics> {
        self.window.statistics()
    }

    /// Intervals currently in the window.
    #[must_use]
    pub const fn samples(&self) -> usize {
        self.window.len()
    }

    /// Time of the last recorded heartbeat.
    #[must_use]
    pub const fn last_heartbeat(&self) -> Option<Duration> {
        match self.window.last() {
            Some(last) => Some(Duration::from_nanos(last)),
            None => None,
        }
    }

    /// The policy this detector was built with.
    #[must_use]
    pub const fn config(&self) -> Config {
        self.config
    }

    /// Forgets every interval and the last heartbeat; warm-up starts over.
    pub const fn reset(&mut self) {
        self.window.reset();
    }
}
