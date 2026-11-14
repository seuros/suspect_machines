//! Sliding window of heartbeat intervals for one peer.

use core::time::Duration;

use crate::{Beat, Config, Refusal, Statistics, Suspicion, duration_to_f64, half_range, math};

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

    /// A window whose first heartbeat was at `now`: what [`Window::observe`]
    /// makes of an empty window, without a refusal arm it could never take.
    pub const fn started(now: u64) -> Self {
        let mut window = Self::new();
        window.last = Some(now);
        window
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
        // `next < W` whenever `W > 0`, so the slot is missing only for a
        // window of no slots, where an interval enters and leaves at once.
        // No public type builds one: `Config::fit` refuses `W = 0`.
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
        let stored = self.intervals.iter().take(self.len);
        let &first = stored.clone().next()?;
        let (mut min, mut max) = (first, first);
        for &interval in stored.clone() {
            min = min.min(interval);
            max = max.max(interval);
        }
        let (min, max) = (Duration::from_nanos(min), Duration::from_nanos(max));
        let (low, high) = (duration_to_f64(min), duration_to_f64(max));
        let count = count_to_f64(self.len);
        // The exact mean lies within min..=max and the exact deviation within
        // half the range; rounding may step past either bound, so clamp back
        // to what the intervals can give.
        let mean_ns = (wide_to_f64(self.sum) / count).max(low).min(high);
        let mut squares = 0.0;
        for &interval in stored {
            let deviation = to_f64(interval) - mean_ns;
            squares += deviation * deviation;
        }
        Some(Statistics {
            samples: self.len,
            mean_ns,
            std_dev_ns: math::sqrt(squares / count).min(half_range(low, high)),
            min,
            max,
        })
    }

    /// The suspicion at `now` nanoseconds under `config`; `None` when `now`
    /// precedes the last heartbeat, so there is no silence to measure. The
    /// callers answer that as [`Refusal::TimeRegression`] or skip the peer.
    pub fn suspicion(&self, config: &Config, now: u64) -> Option<Suspicion> {
        let Some(last) = self.last else {
            return Some(Suspicion::NoHeartbeat);
        };
        let silence = now.checked_sub(last)?;
        Some(match self.statistics() {
            Some(statistics) if self.len >= config.min_samples() => {
                Suspicion::Measured(config.phi(&statistics, silence))
            }
            Some(_) | None => Suspicion::WarmingUp {
                samples: self.len,
                required: config.min_samples(),
                silence: Duration::from_nanos(silence),
            },
        })
    }
}
