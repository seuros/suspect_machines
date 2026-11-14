//! Ruby `Data` values to core types and back.
use std::time::Duration;

use magnus::{Error, Ruby, Symbol, Value, prelude::*};
use matryona_machines::ruby::{duration, exact_fields, integer, nanoseconds, symbol};
use suspect_machines::{
    Beat, Config, ConfigError, Distribution, Normal, Refusal, Statistics, Suspicion,
};

use crate::classes::classes;
use crate::{phi_value, refusal};

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

/// `:exponential`, or a `Normal` carrying its deviation floor.
pub fn distribution(ruby: &Ruby, value: Value) -> Result<Distribution, Error> {
    if let Some(symbol) = Symbol::from_value(value) {
        return match symbol.name()?.as_ref() {
            "exponential" => Ok(Distribution::Exponential),
            _ => Err(Error::new(
                ruby.exception_range_error(),
                "distribution must be :exponential or a SuspectMachines::Normal",
            )),
        };
    }
    let message = "distribution must be :exponential or a SuspectMachines::Normal";
    let [floor] = exact_fields(ruby, value, classes(ruby)?.normal.get(ruby), message)?;
    let floor = duration(floor, "min_std_dev")?;
    normal(ruby, floor).map(Distribution::Normal)
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
        Distribution::Exponential => Ok(symbol(ruby, "exponential").as_value()),
    }
}

pub fn config(ruby: &Ruby, value: Value) -> Result<Config, Error> {
    let message = "expected a SuspectMachines::Config";
    let [distribution, min_samples, pause] =
        exact_fields(ruby, value, classes(ruby)?.config.get(ruby), message)?;
    let min_samples = integer(min_samples, "min_samples")?;
    let pause = duration(pause, "acceptable_pause")?;
    config_from(ruby, distribution, min_samples, pause)
}

/// The core's `Config::new` then `Config::with_acceptable_pause`.
pub fn config_from(
    ruby: &Ruby,
    distribution: Value,
    min_samples: usize,
    pause: Duration,
) -> Result<Config, Error> {
    let distribution = self::distribution(ruby, distribution)?;
    Config::new(distribution, min_samples)
        .and_then(|config| config.with_acceptable_pause(pause))
        .map_err(|error| config_error(ruby, error))
}

pub fn config_value(ruby: &Ruby, config: Config) -> Result<Value, Error> {
    classes(ruby)?.config.get(ruby).funcall(
        "__build",
        (
            distribution_value(ruby, config.distribution())?,
            config.min_samples(),
            nanoseconds(ruby, config.acceptable_pause()),
        ),
    )
}

/// An answer of `heartbeat`; a `Recorded` is built through `new`. The
/// interval is a difference of two `u64` instants, so it always fits.
pub fn beat(ruby: &Ruby, outcome: Result<Beat, Refusal>) -> Result<Value, Error> {
    let name = match outcome {
        Ok(Beat::First) => "first",
        Ok(Beat::Recorded { interval, samples }) => {
            let recorded = classes(ruby)?.recorded.get(ruby);
            return recorded.funcall("new", (nanoseconds(ruby, interval), samples));
        }
        Ok(Beat::Restarted) => "restarted",
        Err(refused) => return Ok(refusal(ruby, refused)),
    };
    Ok(symbol(ruby, name).as_value())
}

pub fn suspicion(ruby: &Ruby, outcome: Result<Suspicion, Refusal>) -> Result<Value, Error> {
    match outcome {
        Ok(Suspicion::NoHeartbeat) => Ok(symbol(ruby, "no_heartbeat").as_value()),
        Ok(Suspicion::WarmingUp {
            samples,
            required,
            silence,
        }) => classes(ruby)?
            .warming_up
            .get(ruby)
            .funcall("new", (samples, required, nanoseconds(ruby, silence))),
        Ok(Suspicion::Measured(phi)) => classes(ruby)?
            .measured
            .get(ruby)
            .funcall("new", (phi_value(ruby, phi)?,)),
        Err(refused) => Ok(refusal(ruby, refused)),
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
