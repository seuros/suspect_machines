//! Sliding window of heartbeat intervals for one peer.

use core::time::Duration;

use crate::{Beat, Config, Refusal, Statistics, Suspicion, math};

/// Nanoseconds as `f64`. Exact up to `2^53` ns (about 104 days); beyond that
/// the nearest representable value is used.
#[allow(
    clippy::cast_precision_loss,
    reason = "documented rounding above 2^53 ns"
)]
pub const fn to_f64(value: u64) -> f64 {
    value as f64
}

#[allow(
    clippy::cast_precision_loss,
    reason = "a sum of u64 intervals rounds to the nearest f64"
)]
const fn wide_to_f64(value: u128) -> f64 {
    value as f64
}

#[allow(
    clippy::cast_precision_loss,
    reason = "sample counts are far below 2^53"
)]
const fn count_to_f64(value: usize) -> f64 {
    value as f64
}

/// The last `W` intervals, their exact sum and the last heartbeat time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window<const W: usize> {
    intervals: [u64; W],
    next: usize,
    len: usize,
    sum: u128,
    last: Option<u64>,
}

impl<const W: usize> Window<W> {
    pub const fn new() -> Self {
        Self {
            intervals: [0; W],
            next: 0,
            len: 0,
            sum: 0,
            last: None,
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn last(&self) -> Option<u64> {
        self.last
    }

    pub const fn reset(&mut self) {
        *self = Self::new();
    }

    /// Records a heartbeat at `now` nanoseconds. Refusals leave the window
    /// untouched.
    pub fn observe(&mut self, now: u64) -> Result<Beat, Refusal> {
        let Some(last) = self.last else {
            self.last = Some(now);
            return Ok(Beat::First);
        };
        if now < last {
            return Err(Refusal::TimeRegression);
        }
        if now == last {
            return Err(Refusal::SameInstant);
        }
        let interval = now - last;
        if let Some(slot) = self.intervals.get_mut(self.next) {
            if self.len == W {
                self.sum -= u128::from(*slot);
            } else {
                self.len += 1;
            }
            *slot = interval;
            self.sum += u128::from(interval);
            self.next = (self.next + 1) % W;
        }
        self.last = Some(now);
        Ok(Beat::Recorded {
            interval: Duration::from_nanos(interval),
            samples: self.len,
        })
    }

    /// Statistics of the stored intervals; `None` while there are none.
    pub fn statistics(&self) -> Option<Statistics> {
        let stored = self.intervals.get(..self.len)?;
        let (&first, _) = stored.split_first()?;
        let count = count_to_f64(self.len);
        let mean_ns = wide_to_f64(self.sum) / count;
        let mut squares = 0.0;
        let mut min = first;
        let mut max = first;
        for &interval in stored {
            let deviation = to_f64(interval) - mean_ns;
            squares += deviation * deviation;
            min = min.min(interval);
            max = max.max(interval);
        }
        Some(Statistics {
            samples: self.len,
            mean_ns,
            std_dev_ns: math::sqrt(squares / count),
            min: Duration::from_nanos(min),
            max: Duration::from_nanos(max),
        })
    }

    /// The suspicion at `now` nanoseconds under `config`.
    pub fn suspicion(&self, config: &Config, now: u64) -> Result<Suspicion, Refusal> {
        let Some(last) = self.last else {
            return Ok(Suspicion::NoHeartbeat);
        };
        if now < last {
            return Err(Refusal::TimeRegression);
        }
        let silence = now - last;
        match self.statistics() {
            Some(statistics) if self.len >= config.min_samples() => {
                Ok(Suspicion::Measured(config.phi(&statistics, silence)))
            }
            Some(_) | None => Ok(Suspicion::WarmingUp {
                samples: self.len,
                required: config.min_samples(),
                silence: Duration::from_nanos(silence),
            }),
        }
    }
}
