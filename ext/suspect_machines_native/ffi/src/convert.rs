//! Ruby `Data` values to core types and back.
use magnus::{Error, Ruby, Symbol, Value, prelude::*};
use suspect_machines::{
    Beat, Config, ConfigError, Distribution, Normal, Refusal, Statistics, Suspicion,
};

use crate::classes::{classes, exact, field};
use crate::{Number, nanoseconds, phi_value, refusal};

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
    let floor: Number = field(exact(ruby, value, classes(ruby)?.normal, message)?, 0)?;
    normal(ruby, floor).map(Distribution::Normal)
}

/// The core's `Normal::new`: `RangeError` for a zero floor.
pub fn normal(ruby: &Ruby, min_std_dev: Number) -> Result<Normal, Error> {
    Normal::new(min_std_dev.duration()).map_err(|error| config_error(ruby, error))
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
            let floor = nanoseconds(ruby, normal.min_std_dev())?;
            ruby.get_inner(classes(ruby)?.normal)
                .funcall("new", (floor,))
        }
        Distribution::Exponential => Ok(ruby.to_symbol("exponential").as_value()),
    }
}

pub fn config(ruby: &Ruby, value: Value) -> Result<Config, Error> {
    let held = exact(
        ruby,
        value,
        classes(ruby)?.config,
        "expected a SuspectMachines::Config",
    )?;
    let (min_samples, pause): (Number, Number) = (field(held, 1)?, field(held, 2)?);
    config_from(ruby, field(held, 0)?, min_samples, pause)
}

/// The core's `Config::new` then `Config::with_acceptable_pause`.
pub fn config_from(
    ruby: &Ruby,
    distribution: Value,
    min_samples: Number,
    pause: Number,
) -> Result<Config, Error> {
    let distribution = self::distribution(ruby, distribution)?;
    Config::new(distribution, min_samples.index(ruby)?)
        .and_then(|config| config.with_acceptable_pause(pause.duration()))
        .map_err(|error| config_error(ruby, error))
}

pub fn config_value(ruby: &Ruby, config: Config) -> Result<Value, Error> {
    ruby.get_inner(classes(ruby)?.config).funcall(
        "__build",
        (
            distribution_value(ruby, config.distribution())?,
            config.min_samples(),
            nanoseconds(ruby, config.acceptable_pause())?,
        ),
    )
}

/// An answer of `heartbeat`; a `Recorded` is built through `new`. The
/// interval is a difference of two `u64` instants, so it always fits.
pub fn beat(ruby: &Ruby, outcome: Result<Beat, Refusal>) -> Result<Value, Error> {
    let name = match outcome {
        Ok(Beat::First) => "first",
        Ok(Beat::Recorded { interval, samples }) => {
            let recorded = ruby.get_inner(classes(ruby)?.recorded);
            return recorded.funcall("new", (nanoseconds(ruby, interval)?, samples));
        }
        Ok(Beat::Restarted) => "restarted",
        Err(refused) => return Ok(refusal(ruby, refused)),
    };
    Ok(ruby.to_symbol(name).as_value())
}

pub fn suspicion(ruby: &Ruby, outcome: Result<Suspicion, Refusal>) -> Result<Value, Error> {
    match outcome {
        Ok(Suspicion::NoHeartbeat) => Ok(ruby.to_symbol("no_heartbeat").as_value()),
        Ok(Suspicion::WarmingUp {
            samples,
            required,
            silence,
        }) => ruby
            .get_inner(classes(ruby)?.warming_up)
            .funcall("new", (samples, required, nanoseconds(ruby, silence)?)),
        Ok(Suspicion::Measured(phi)) => ruby
            .get_inner(classes(ruby)?.measured)
            .funcall("new", (phi_value(ruby, phi)?,)),
        Err(refused) => Ok(refusal(ruby, refused)),
    }
}

pub fn statistics(ruby: &Ruby, statistics: Option<Statistics>) -> Result<Option<Value>, Error> {
    let Some(statistics) = statistics else {
        return Ok(None);
    };
    ruby.get_inner(classes(ruby)?.statistics)
        .funcall(
            "new",
            (
                statistics.samples(),
                statistics.mean_ns(),
                statistics.std_dev_ns(),
                nanoseconds(ruby, statistics.min())?,
                nanoseconds(ruby, statistics.max())?,
            ),
        )
        .map(Some)
}
