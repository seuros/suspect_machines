//! The private hooks every `Data` value calls on construction: each checks
//! its fields through the core and answers what the value stores. Wrong type
//! is `TypeError`; a value the core would never hold is `RangeError`.
use std::time::Duration;

use magnus::{Error, RArray, Ruby, Value, prelude::*};
use matryona_machines::ruby::{exact_array, exact_fields, finite, integer, optional};
use suspect_machines::{Beat, Scan, Statistics, Suspect, Suspicion};

use crate::classes::classes;
use crate::{CAPACITY, WINDOW, convert, phi, phi_from, range_error, threshold, threshold_from};

/// `Phi.new`: the level as a Float the core's `Phi::new` accepts.
pub fn phi_level(ruby: &Ruby, value: Value) -> Result<f64, Error> {
    phi(ruby, finite(value, "phi")?).map(suspect_machines::Phi::value)
}

/// `Threshold.new`: the level as a Float the core's `Threshold::new` accepts.
pub fn threshold_level(ruby: &Ruby, value: Value) -> Result<f64, Error> {
    let level = finite(value, "threshold")?;
    threshold(ruby, level).map(suspect_machines::Threshold::value)
}

/// `Phi#millis`: the core's `Phi::millis`.
pub fn millis(ruby: &Ruby, this: Value) -> Result<u64, Error> {
    phi_from(ruby, this).map(suspect_machines::Phi::millis)
}

/// `Phi#reaches?`: the core's `Phi::reaches`, for a `Threshold` only.
pub fn reaches(ruby: &Ruby, this: Value, threshold: Value) -> Result<bool, Error> {
    let threshold = threshold_from(ruby, threshold)?;
    Ok(phi_from(ruby, this)?.reaches(threshold))
}

/// `Normal.new`: the core's `Normal::new`, `RangeError` for a zero floor.
pub fn normal(ruby: &Ruby, min_std_dev: Value) -> Result<u64, Error> {
    let floor: u64 = integer(min_std_dev, "min_std_dev")?;
    convert::normal(ruby, Duration::from_nanos(floor)).map(|_| floor)
}

/// `Config.new`, `with_acceptable_pause` and `with_warm_up_ceiling`: the
/// core's `Config::new`, `Config::with_acceptable_pause` and
/// `Config::with_warm_up_ceiling`.
pub fn config(
    ruby: &Ruby,
    distribution: Value,
    min_samples: Value,
    pause: Value,
    ceiling: Value,
) -> Result<(Value, usize, u64, Value), Error> {
    let min_samples = integer(min_samples, "min_samples")?;
    let pause: u64 = integer(pause, "acceptable_pause")?;
    let held = convert::ceiling_from(ceiling)?;
    convert::config_from(
        ruby,
        distribution,
        min_samples,
        Duration::from_nanos(pause),
        held,
    )?;
    Ok((distribution, min_samples, pause, ceiling))
}

/// `Config#with_acceptable_pause`: the core's `Config::with_acceptable_pause`
/// on this config, read by its exact class from the struct (no reader runs)
/// and revalidated through the core, since `Marshal.load` skips
/// `initialize`. The copy is built from what the core answered.
pub fn with_acceptable_pause(ruby: &Ruby, this: Value, pause: Value) -> Result<Value, Error> {
    let config = convert::config(ruby, this)?;
    let pause: u64 = integer(pause, "acceptable_pause")?;
    let paused = config
        .with_acceptable_pause(Duration::from_nanos(pause))
        .map_err(|error| convert::config_error(ruby, error))?;
    convert::config_value(ruby, paused)
}

/// `Config#with_warm_up_ceiling`: the core's `Config::with_warm_up_ceiling`
/// on this config, read and revalidated as `with_acceptable_pause` does.
pub fn with_warm_up_ceiling(ruby: &Ruby, this: Value, ceiling: Value) -> Result<Value, Error> {
    let config = convert::config(ruby, this)?;
    let ceiling: u64 = integer(ceiling, "warm_up_ceiling")?;
    let ceiled = config
        .with_warm_up_ceiling(Duration::from_nanos(ceiling))
        .map_err(|error| convert::config_error(ruby, error))?;
    convert::config_value(ruby, ceiled)
}

/// `Recorded.new`: the core's `Beat::recorded` for the compiled window.
pub fn recorded(ruby: &Ruby, interval: Value, samples: Value) -> Result<(u64, usize), Error> {
    let interval: u64 = integer(interval, "interval")?;
    let samples = integer(samples, "samples")?;
    Beat::recorded::<WINDOW>(Duration::from_nanos(interval), samples)
        .map(|_| (interval, samples))
        .ok_or_else(|| {
            range_error(
                ruby,
                "a Recorded needs interval >= 1 and samples in 1..WINDOW",
            )
        })
}

/// `WarmingUp.new`: the core's `Suspicion::warming_up` for the compiled
/// window.
pub fn warming_up(
    ruby: &Ruby,
    samples: Value,
    required: Value,
    silence: Value,
) -> Result<(usize, usize, u64), Error> {
    let (samples, required) = (integer(samples, "samples")?, integer(required, "required")?);
    let silence: u64 = integer(silence, "silence")?;
    Suspicion::warming_up::<WINDOW>(samples, required, Duration::from_nanos(silence))
        .map(|_| (samples, required, silence))
        .ok_or_else(|| range_error(ruby, "a WarmingUp needs samples < required <= WINDOW"))
}

/// `Measured.new`: the level must be a `Phi`.
pub fn measured(ruby: &Ruby, phi: Value) -> Result<Value, Error> {
    phi_from(ruby, phi).map(|_| phi)
}

/// `Statistics.new`: the core's `Statistics::new` for the compiled window.
pub fn statistics(
    ruby: &Ruby,
    samples: Value,
    mean_ns: Value,
    std_dev_ns: Value,
    min: Value,
    max: Value,
) -> Result<(usize, f64, f64, u64, u64), Error> {
    let samples = integer(samples, "samples")?;
    let mean = finite(mean_ns, "mean_ns")?;
    let spread = finite(std_dev_ns, "std_dev_ns")?;
    let (min, max): (u64, u64) = (integer(min, "min")?, integer(max, "max")?);
    let (low, high) = (Duration::from_nanos(min), Duration::from_nanos(max));
    Statistics::new::<WINDOW>(samples, mean, spread, low, high)
        .map(|stats| (samples, stats.mean_ns(), stats.std_dev_ns(), min, max))
        .ok_or_else(|| {
            range_error(
                ruby,
                "Statistics need samples in 1..WINDOW, 1 <= min <= max (equal for one \
                 sample), a mean within min..max and a positively signed deviation of \
                 at most (max - min) / 2",
            )
        })
}

/// `Suspect.new`: any `u64` peer and incarnation, and a `Measured` or a
/// `WarmingUp`, as the core's `Suspect::new` accepts.
pub fn suspect(
    ruby: &Ruby,
    peer: Value,
    incarnation: Value,
    suspicion: Value,
) -> Result<(u64, u64, Value), Error> {
    let (peer, incarnation) = (integer(peer, "peer")?, integer(incarnation, "incarnation")?);
    let held = convert::suspect_suspicion(ruby, suspicion)?;
    Suspect::new(peer, incarnation, held)
        .ok_or_else(|| range_error(ruby, "a Suspect is a Measured or an unfinished WarmingUp"))?;
    Ok((peer, incarnation, suspicion))
}

/// `Scan.new`: an Array of `Suspect` and a resume cursor, as the core's
/// `Scan::new` for the compiled capacity accepts. Stores a frozen copy.
pub fn scan(ruby: &Ruby, suspects: Value, next: Value) -> Result<(RArray, Option<usize>), Error> {
    let message = "suspects must be an Array of SuspectMachines::Suspect";
    let list = exact_array(ruby, suspects, message)?;
    let copy = ruby.ary_new_capa(list.len());
    let class = classes(ruby)?.suspect.get(ruby);
    for entry in list {
        let message = "suspects must be SuspectMachines::Suspect";
        let [peer, incarnation, suspicion] = exact_fields(ruby, entry, class, message)?;
        suspect(ruby, peer, incarnation, suspicion)?;
        copy.push(entry)?;
    }
    let cursor: Option<usize> = optional(next, "next")?;
    Scan::new::<CAPACITY>(copy.len(), cursor).ok_or_else(|| {
        range_error(
            ruby,
            "a Scan holds at most CAPACITY suspects and resumes below CAPACITY",
        )
    })?;
    copy.freeze();
    Ok((copy, cursor))
}
