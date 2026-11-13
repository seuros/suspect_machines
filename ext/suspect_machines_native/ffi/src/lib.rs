//! Thin binding to the phi-accrual detector and the bounded roster.
mod convert;
mod detector;
mod init;
mod roster;

use std::time::Duration;

use magnus::{Error, Float, Integer, RClass, Ruby, TryConvert, Value, prelude::*};
use suspect_machines::{Refusal, Threshold};

/// Peers one roster can track.
const CAPACITY: usize = 64;
/// Intervals kept per peer, by a roster and by a detector alike.
const WINDOW: usize = 128;

#[derive(Clone, Copy)]
struct Number(u64);

impl TryConvert for Number {
    fn try_convert(value: Value) -> Result<Self, Error> {
        if Integer::from_value(value).is_none() {
            return Err(Error::new(
                Ruby::get_with(value).exception_type_error(),
                "expected an Integer (unsigned 64-bit)",
            ));
        }
        u64::try_convert(value).map(Self)
    }
}

impl Number {
    fn index(self, ruby: &Ruby) -> Result<usize, Error> {
        usize::try_from(self.0)
            .map_err(|_| Error::new(ruby.exception_range_error(), "count out of range"))
    }

    /// The Integer as a time: nanoseconds.
    const fn duration(self) -> Duration {
        Duration::from_nanos(self.0)
    }
}

/// A phi threshold: the core's `Threshold`, a finite Float above zero.
#[derive(Clone, Copy)]
struct Level(Threshold);

impl TryConvert for Level {
    fn try_convert(value: Value) -> Result<Self, Error> {
        let ruby = Ruby::get_with(value);
        if Integer::from_value(value).is_none() && Float::from_value(value).is_none() {
            return Err(Error::new(
                ruby.exception_type_error(),
                "expected a Float or Integer phi threshold",
            ));
        }
        Threshold::new(f64::try_convert(value)?)
            .map(Self)
            .ok_or_else(|| {
                Error::new(
                    ruby.exception_range_error(),
                    "phi threshold must be finite and greater than zero",
                )
            })
    }
}

/// Every variant has its own symbol. `Overflow` cannot be reached from Ruby:
/// every Integer the binding accepts fits `u64` nanoseconds.
fn refusal(ruby: &Ruby, refusal: Refusal) -> Value {
    let name = match refusal {
        Refusal::TimeRegression => "time_regression",
        Refusal::SameInstant => "same_instant",
        Refusal::Overflow => "overflow",
        Refusal::Stale => "stale",
        Refusal::Full => "full",
        Refusal::Untracked => "untracked",
    };
    ruby.to_symbol(name).as_value()
}

fn nanoseconds(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn data(ruby: &Ruby, name: &str) -> Result<RClass, Error> {
    ruby.define_module("SuspectMachines")?.const_get(name)
}
