//! The private hooks every `Data` value calls on construction: each checks
//! its fields through the core and answers what the value stores. Wrong type
//! is `TypeError`; a value the core would never hold is `RangeError`.
use magnus::{Error, RArray, Ruby, Value, prelude::*};
use suspect_machines::{Beat, Scan, Statistics, Suspicion};

use crate::classes::{classes, exact, field};
use crate::{
    CAPACITY, Number, WINDOW, convert, phi, phi_from, range_error, real, threshold, threshold_from,
};

/// `Phi.new`: the level as a Float the core's `Phi::new` accepts.
pub fn phi_level(ruby: &Ruby, value: Value) -> Result<f64, Error> {
    phi(ruby, real(value, "phi must be a Float or Integer")?).map(suspect_machines::Phi::value)
}

/// `Threshold.new`: the level as a Float the core's `Threshold::new` accepts.
pub fn threshold_level(ruby: &Ruby, value: Value) -> Result<f64, Error> {
    let level = real(value, "threshold must be a Float or Integer")?;
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
pub fn normal(ruby: &Ruby, min_std_dev: Number) -> Result<u64, Error> {
    convert::normal(ruby, min_std_dev).map(|_| min_std_dev.0)
}

/// `Config.new` and `with_acceptable_pause`: the core's `Config::new` and
/// `Config::with_acceptable_pause`.
pub fn config(
    ruby: &Ruby,
    distribution: Value,
    min_samples: Number,
    pause: Number,
) -> Result<(Value, u64, u64), Error> {
    convert::config_from(ruby, distribution, min_samples, pause)?;
    Ok((distribution, min_samples.0, pause.0))
}

/// `Recorded.new`: the core's `Beat::recorded` for the compiled window.
pub fn recorded(ruby: &Ruby, interval: Number, samples: Number) -> Result<(u64, u64), Error> {
    Beat::recorded::<WINDOW>(interval.duration(), samples.index(ruby)?)
        .map(|_| (interval.0, samples.0))
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
    samples: Number,
    required: Number,
    silence: Number,
) -> Result<(u64, u64, u64), Error> {
    let (count, needed) = (samples.index(ruby)?, required.index(ruby)?);
    Suspicion::warming_up::<WINDOW>(count, needed, silence.duration())
        .map(|_| (samples.0, required.0, silence.0))
        .ok_or_else(|| range_error(ruby, "a WarmingUp needs samples < required <= WINDOW"))
}

/// `Measured.new`: the level must be a `Phi`.
pub fn measured(ruby: &Ruby, phi: Value) -> Result<Value, Error> {
    phi_from(ruby, phi).map(|_| phi)
}

/// `Statistics.new`: the core's `Statistics::new` for the compiled window.
pub fn statistics(
    ruby: &Ruby,
    samples: Number,
    mean_ns: Value,
    std_dev_ns: Value,
    min: Number,
    max: Number,
) -> Result<(u64, f64, f64, u64, u64), Error> {
    let mean = real(mean_ns, "mean_ns must be a Float or Integer")?;
    let spread = real(std_dev_ns, "std_dev_ns must be a Float or Integer")?;
    let count = samples.index(ruby)?;
    Statistics::new::<WINDOW>(count, mean, spread, min.duration(), max.duration())
        .map(|stats| (samples.0, stats.mean_ns(), stats.std_dev_ns(), min.0, max.0))
        .ok_or_else(|| {
            range_error(
                ruby,
                "Statistics need samples in 1..WINDOW, 1 <= min <= max (equal for one \
                 sample), a mean within min..max and a positively signed deviation of \
                 at most (max - min) / 2",
            )
        })
}

/// `Suspect.new`: any `u64` peer and incarnation, and a `Phi`.
pub fn suspect(
    ruby: &Ruby,
    peer: Number,
    incarnation: Number,
    phi: Value,
) -> Result<(u64, u64, Value), Error> {
    phi_from(ruby, phi)?;
    Ok((peer.0, incarnation.0, phi))
}

/// `Scan.new`: an Array of `Suspect` and a resume cursor, as the core's
/// `Scan::new` for the compiled capacity accepts. Stores a frozen copy.
pub fn scan(ruby: &Ruby, suspects: Value, next: Value) -> Result<(RArray, Option<u64>), Error> {
    let Some(list) = RArray::from_value(suspects) else {
        return Err(Error::new(
            ruby.exception_type_error(),
            "suspects must be an Array of SuspectMachines::Suspect",
        ));
    };
    let copy = ruby.ary_new_capa(list.len());
    let class = classes(ruby)?.suspect;
    for entry in list {
        let held = exact(
            ruby,
            entry,
            class,
            "suspects must be SuspectMachines::Suspect",
        )?;
        let (peer, incarnation): (Number, Number) = (field(held, 0)?, field(held, 1)?);
        suspect(ruby, peer, incarnation, field(held, 2)?)?;
        copy.push(entry)?;
    }
    let cursor: Option<Number> = if next.is_nil() {
        None
    } else {
        Some(magnus::TryConvert::try_convert(next)?)
    };
    let index = cursor.map(|cursor| cursor.index(ruby)).transpose()?;
    Scan::new::<CAPACITY>(copy.len(), index).ok_or_else(|| {
        range_error(
            ruby,
            "a Scan holds at most CAPACITY suspects and resumes below CAPACITY",
        )
    })?;
    copy.freeze();
    Ok((copy, cursor.map(|cursor| cursor.0)))
}
