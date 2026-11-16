//! Ruby `Data` values to core types and back.
use std::time::Duration;

use magnus::{Error, Ruby, Value, prelude::*};
use matryona_machines::ruby::{
    Choice, Refused, SymbolNames, duration, exact_fields, integer, nanoseconds, optional,
    symbol_or_fields_why, which_struct,
};
use suspect_machines::{
    Beat, Config, ConfigError, Distribution, Normal, Refusal, Statistics, Suspicion,
};

use crate::classes::classes;
use crate::{WINDOW, phi_from, phi_value, range_error, refusal};

/// A constructor error of the core, raised as `RangeError`. `DurationOverflow`
/// and `ZeroWindow` cannot be reached from Ruby: times are `u64` nanoseconds
/// and the window is compiled in.
pub fn config_error(ruby: &Ruby, error: ConfigError) -> Error {
    let message = match error {
        ConfigError::ZeroWarmUp => "min_samples must be at least 1",
        ConfigError::ZeroDeviationFloor => "min_std_dev must be at least 1",
        ConfigError::DurationOverflow => "a duration exceeds 2**64-1 nanoseconds",
        ConfigError::ZeroWindow => "the compiled window has no slots",
        ConfigError::WarmUpExceedsWindow => "min_samples exceeds the compiled window",
    };
    Error::new(ruby.exception_range_error(), message)
}

matryona_machines::symbols! {
    /// A distribution as Ruby holds it: `:exponential`, or a `Normal`.
    Distributions for Distribution as "a distribution" => names_only {
        Normal(..) => "normal",
        Exponential => "exponential",
    }
}

matryona_machines::symbols! {
    /// A beat as Ruby holds it: `:first`, a `Recorded`, or `:restarted`.
    Beats for Beat as "a beat" => names_only {
        First => "first",
        Recorded { .. } => "recorded",
        Restarted => "restarted",
    }
}

matryona_machines::symbols! {
    /// A suspicion as Ruby holds it: `:no_heartbeat`, a `WarmingUp`, or a
    /// `Measured`.
    Suspicions for Suspicion as "a suspicion" => names_only {
        NoHeartbeat => "no_heartbeat",
        WarmingUp { .. } => "warming_up",
        Measured(..) => "measured",
    }
}

/// `:exponential`, or a `Normal` carrying its deviation floor: `RangeError`
/// naming any other Symbol (`unknown distribution :nope`), `TypeError` for
/// anything else.
pub fn distribution(ruby: &Ruby, value: Value) -> Result<Distribution, Error> {
    let normal_class = classes(ruby)?.normal.get(ruby);
    let read = symbol_or_fields_why(
        ruby,
        value,
        Distributions,
        &[normal_class],
        |why| match why {
            Refused::UnknownSymbol(symbol) => format!("unknown distribution {}", symbol.inspect()),
            _ => "distribution must be :exponential or a SuspectMachines::Normal".into(),
        },
    )?;
    match read {
        Choice::Symbol(distribution) => Ok(distribution),
        Choice::Struct(_, [floor]) => {
            let floor = duration(floor, "min_std_dev")?;
            normal(ruby, floor).map(Distribution::Normal)
        }
    }
}

/// The core's `Normal::new`: `RangeError` for a zero floor.
pub fn normal(ruby: &Ruby, min_std_dev: Duration) -> Result<Normal, Error> {
    Normal::new(min_std_dev).map_err(|error| config_error(ruby, error))
}

/// A config read from Ruby, handed to the core's constructor `build` for a
/// detector or a roster; either error is `RangeError`.
pub fn configured<T>(
    ruby: &Ruby,
    value: Value,
    build: impl FnOnce(Config) -> Result<T, ConfigError>,
) -> Result<T, Error> {
    build(config(ruby, value)?).map_err(|error| config_error(ruby, error))
}

fn distribution_value(ruby: &Ruby, distribution: Distribution) -> Result<Value, Error> {
    match distribution {
        Distribution::Normal(normal) => {
            let floor = nanoseconds(ruby, normal.min_std_dev());
            classes(ruby)?.normal.get(ruby).funcall("new", (floor,))
        }
        Distribution::Exponential => {
            Ok(Distributions::symbol(ruby, &Distribution::Exponential).as_value())
        }
    }
}

pub fn config(ruby: &Ruby, value: Value) -> Result<Config, Error> {
    let message = "expected a SuspectMachines::Config";
    let [distribution, min_samples, pause, ceiling] =
        exact_fields(ruby, value, classes(ruby)?.config.get(ruby), message)?;
    let min_samples = integer(min_samples, "min_samples")?;
    let pause = duration(pause, "acceptable_pause")?;
    let ceiling = ceiling_from(ceiling)?;
    config_from(ruby, distribution, min_samples, pause, ceiling)
}

/// A warm-up ceiling: `nil` or Integer nanoseconds.
pub fn ceiling_from(value: Value) -> Result<Option<Duration>, Error> {
    let ceiling: Option<u64> = optional(value, "warm_up_ceiling")?;
    Ok(ceiling.map(Duration::from_nanos))
}

/// The core's `Config::new`, `Config::with_acceptable_pause`, then
/// `Config::with_warm_up_ceiling` when there is one.
pub fn config_from(
    ruby: &Ruby,
    distribution: Value,
    min_samples: usize,
    pause: Duration,
    ceiling: Option<Duration>,
) -> Result<Config, Error> {
    let distribution = self::distribution(ruby, distribution)?;
    Config::new(distribution, min_samples)
        .and_then(|config| config.with_acceptable_pause(pause))
        .and_then(|config| {
            ceiling.map_or(Ok(config), |ceiling| config.with_warm_up_ceiling(ceiling))
        })
        .map_err(|error| config_error(ruby, error))
}

pub fn config_value(ruby: &Ruby, config: Config) -> Result<Value, Error> {
    classes(ruby)?.config.get(ruby).funcall(
        "__build",
        (
            distribution_value(ruby, config.distribution())?,
            config.min_samples(),
            nanoseconds(ruby, config.acceptable_pause()),
            config
                .warm_up_ceiling()
                .map(|ceiling| nanoseconds(ruby, ceiling)),
        ),
    )
}

/// An answer of `heartbeat`; a `Recorded` is built through `new`. The
/// interval is a difference of two `u64` instants, so it always fits.
pub fn beat(ruby: &Ruby, outcome: Result<Beat, Refusal>) -> Result<Value, Error> {
    match outcome {
        Ok(Beat::Recorded { interval, samples }) => {
            let recorded = classes(ruby)?.recorded.get(ruby);
            recorded.funcall("new", (nanoseconds(ruby, interval), samples))
        }
        Ok(beat @ (Beat::First | Beat::Restarted)) => Ok(Beats::symbol_of(ruby, beat).as_value()),
        Err(refused) => Ok(refusal(ruby, refused)),
    }
}

/// The core's `Suspicion` a `Suspect` carries, rebuilt from a `WarmingUp`
/// or a `Measured` read by its exact class and checked again.
pub fn suspect_suspicion(ruby: &Ruby, value: Value) -> Result<Suspicion, Error> {
    let message = "suspicion must be a SuspectMachines::WarmingUp or a SuspectMachines::Measured";
    let classes = classes(ruby)?;
    let held_by = [classes.warming_up.get(ruby), classes.measured.get(ruby)];
    let (index, held) = which_struct(ruby, value, &held_by, message)?;
    if index == 0 {
        let [samples, required, silence] = held.fields::<3>(ruby, message)?;
        let (samples, required) = (integer(samples, "samples")?, integer(required, "required")?);
        let silence = duration(silence, "silence")?;
        Suspicion::warming_up::<WINDOW>(samples, required, silence)
            .ok_or_else(|| range_error(ruby, "a WarmingUp needs samples < required <= WINDOW"))
    } else {
        let [phi] = held.fields::<1>(ruby, message)?;
        phi_from(ruby, phi).map(Suspicion::Measured)
    }
}

pub fn suspicion(ruby: &Ruby, outcome: Result<Suspicion, Refusal>) -> Result<Value, Error> {
    match outcome {
        Ok(suspicion) => suspicion_value(ruby, suspicion),
        Err(refused) => Ok(refusal(ruby, refused)),
    }
}

/// A suspicion as Ruby holds it: `:no_heartbeat`, a `WarmingUp` or a
/// `Measured`, each built through `new`.
pub fn suspicion_value(ruby: &Ruby, suspicion: Suspicion) -> Result<Value, Error> {
    match suspicion {
        Suspicion::NoHeartbeat => {
            Ok(Suspicions::symbol_of(ruby, Suspicion::NoHeartbeat).as_value())
        }
        Suspicion::WarmingUp {
            samples,
            required,
            silence,
        } => classes(ruby)?
            .warming_up
            .get(ruby)
            .funcall("new", (samples, required, nanoseconds(ruby, silence))),
        Suspicion::Measured(phi) => classes(ruby)?
            .measured
            .get(ruby)
            .funcall("new", (phi_value(ruby, phi)?,)),
    }
}

pub fn statistics(ruby: &Ruby, statistics: Option<Statistics>) -> Result<Option<Value>, Error> {
    let Some(statistics) = statistics else {
        return Ok(None);
    };
    classes(ruby)?
        .statistics
        .get(ruby)
        .funcall(
            "new",
            (
                statistics.samples(),
                statistics.mean_ns(),
                statistics.std_dev_ns(),
                nanoseconds(ruby, statistics.min()),
                nanoseconds(ruby, statistics.max()),
            ),
        )
        .map(Some)
}
