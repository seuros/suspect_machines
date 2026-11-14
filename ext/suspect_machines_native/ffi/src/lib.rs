//! Thin binding to the phi-accrual detector and the bounded roster.
mod classes;
mod convert;
mod detector;
mod init;
mod roster;
mod values;

use std::time::Duration;

use classes::{classes, exact, field};
use magnus::{Error, Float, Integer, Ruby, TryConvert, Value, prelude::*};
use suspect_machines::{Phi, Refusal, Threshold};

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

/// A Float or Integer as `f64`; anything else is `TypeError`.
fn real(value: Value, what: &'static str) -> Result<f64, Error> {
    if Integer::from_value(value).is_none() && Float::from_value(value).is_none() {
        return Err(Error::new(
            Ruby::get_with(value).exception_type_error(),
            what,
        ));
    }
    f64::try_convert(value)
}

fn range_error(ruby: &Ruby, message: &'static str) -> Error {
    Error::new(ruby.exception_range_error(), message)
}

/// The core's `Phi::new`: `RangeError` unless finite and not negative.
fn phi(ruby: &Ruby, value: f64) -> Result<Phi, Error> {
    Phi::new(value).ok_or_else(|| range_error(ruby, "phi must be finite and not negative"))
}

/// The core's `Threshold::new`: `RangeError` unless finite and above zero.
fn threshold(ruby: &Ruby, value: f64) -> Result<Threshold, Error> {
    Threshold::new(value)
        .ok_or_else(|| range_error(ruby, "threshold must be finite and greater than zero"))
}

/// The core's `Phi` rebuilt from a `SuspectMachines::Phi`.
fn phi_from(ruby: &Ruby, value: Value) -> Result<Phi, Error> {
    let held = exact(
        ruby,
        value,
        classes(ruby)?.phi,
        "expected a SuspectMachines::Phi",
    )?;
    phi(
        ruby,
        real(field(held, 0)?, "phi must be a Float or Integer")?,
    )
}

/// The core's `Threshold` rebuilt from a `SuspectMachines::Threshold`.
fn threshold_from(ruby: &Ruby, value: Value) -> Result<Threshold, Error> {
    let message = "expected a SuspectMachines::Threshold";
    let held = exact(ruby, value, classes(ruby)?.threshold, message)?;
    threshold(
        ruby,
        real(field(held, 0)?, "threshold must be a Float or Integer")?,
    )
}

/// A `SuspectMachines::Phi` holding `phi`, built through `new`.
fn phi_value(ruby: &Ruby, phi: Phi) -> Result<Value, Error> {
    ruby.get_inner(classes(ruby)?.phi)
        .funcall("new", (phi.value(),))
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

/// A core `Duration` as Integer nanoseconds. Every duration the core answers
/// came in as `u64` nanoseconds, so the `RangeError` is never raised in
/// practice; it exists so that no value is ever silently clamped.
fn nanoseconds(ruby: &Ruby, duration: Duration) -> Result<u64, Error> {
    u64::try_from(duration.as_nanos())
        .map_err(|_| range_error(ruby, "a duration exceeds 2**64-1 nanoseconds"))
}

/// An optional core `Duration` as Integer nanoseconds or nil.
fn nanoseconds_or_nil(ruby: &Ruby, duration: Option<Duration>) -> Result<Option<u64>, Error> {
    duration
        .map(|duration| nanoseconds(ruby, duration))
        .transpose()
}
