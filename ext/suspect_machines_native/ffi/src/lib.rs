//! Thin binding to the phi-accrual detector and the bounded roster.
mod classes;
mod convert;
mod detector;
mod init;
mod roster;
mod values;

use classes::classes;
use magnus::{Error, Ruby, Value, prelude::*};
use matryona_machines::ruby::{Symbols, exact_fields, finite};
use suspect_machines::{Phi, Refusal, Threshold};

/// Peers one roster can track.
const CAPACITY: usize = 64;
/// Intervals kept per peer, by a roster and by a detector alike.
const WINDOW: usize = 128;

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
    let message = "expected a SuspectMachines::Phi";
    let [level] = exact_fields(ruby, value, classes(ruby)?.phi.get(ruby), message)?;
    phi(ruby, finite(level, "phi")?)
}

/// The core's `Threshold` rebuilt from a `SuspectMachines::Threshold`.
fn threshold_from(ruby: &Ruby, value: Value) -> Result<Threshold, Error> {
    let message = "expected a SuspectMachines::Threshold";
    let [level] = exact_fields(ruby, value, classes(ruby)?.threshold.get(ruby), message)?;
    threshold(ruby, finite(level, "threshold")?)
}

/// A `SuspectMachines::Phi` holding `phi`, built through `new`.
fn phi_value(ruby: &Ruby, phi: Phi) -> Result<Value, Error> {
    classes(ruby)?.phi.get(ruby).funcall("new", (phi.value(),))
}

matryona_machines::symbols! {
    /// Every variant has its own symbol. `Overflow` cannot be reached from
    /// Ruby: every Integer the binding accepts fits `u64` nanoseconds.
    Refusals for Refusal as "a refusal" {
        TimeRegression => "time_regression",
        SameInstant => "same_instant",
        Overflow => "overflow",
        Stale => "stale",
        Full => "full",
        Untracked => "untracked",
    }
}

/// The refusal's symbol.
fn refusal(ruby: &Ruby, refusal: Refusal) -> Value {
    Refusals::symbol(ruby, refusal).as_value()
}
