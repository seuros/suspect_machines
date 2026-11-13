//! Ruby `Data` values to core types and back.
use magnus::{Error, Ruby, Symbol, Value, prelude::*};
use suspect_machines::{Beat, Config, ConfigError, Distribution, Refusal, Statistics, Suspicion};

use crate::{Number, data, nanoseconds, refusal};

fn type_error(ruby: &Ruby, message: &'static str) -> Error {
    Error::new(ruby.exception_type_error(), message)
}

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
                ruby.exception_arg_error(),
                "distribution must be :exponential or a SuspectMachines::Normal",
            )),
        };
    }
    if !value.is_kind_of(data(ruby, "Normal")?) {
        return Err(type_error(
            ruby,
            "distribution must be :exponential or a SuspectMachines::Normal",
        ));
    }
    let floor: Number = value.funcall("min_std_dev", ())?;
    Ok(Distribution::Normal {
        min_std_dev: floor.duration(),
    })
}

fn distribution_value(ruby: &Ruby, distribution: Distribution) -> Result<Value, Error> {
    match distribution {
        Distribution::Normal { min_std_dev } => {
            data(ruby, "Normal")?.funcall("new", (nanoseconds(min_std_dev),))
        }
        Distribution::Exponential => Ok(ruby.to_symbol("exponential").as_value()),
    }
}

pub fn config(ruby: &Ruby, value: Value) -> Result<Config, Error> {
    if !value.is_kind_of(data(ruby, "Config")?) {
        return Err(type_error(ruby, "expected a SuspectMachines::Config"));
    }
    let distribution = distribution(ruby, value.funcall("distribution", ())?)?;
    let (min_samples, pause): (Number, Number) = (
        value.funcall("min_samples", ())?,
        value.funcall("acceptable_pause", ())?,
    );
    Config::new(distribution, min_samples.index(ruby)?)
        .and_then(|config| config.with_acceptable_pause(pause.duration()))
        .map_err(|error| config_error(ruby, error))
}

pub fn config_value(ruby: &Ruby, config: Config) -> Result<Value, Error> {
    data(ruby, "Config")?.funcall(
        "__build",
        (
            distribution_value(ruby, config.distribution())?,
            config.min_samples(),
            nanoseconds(config.acceptable_pause()),
        ),
    )
}

pub fn beat(ruby: &Ruby, outcome: Result<Beat, Refusal>) -> Result<Value, Error> {
    let name = match outcome {
        Ok(Beat::First) => "first",
        Ok(Beat::Recorded { interval, samples }) => {
            return data(ruby, "Recorded")?.funcall("new", (nanoseconds(interval), samples));
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
        }) => data(ruby, "WarmingUp")?.funcall("new", (samples, required, nanoseconds(silence))),
        Ok(Suspicion::Measured(phi)) => data(ruby, "Measured")?.funcall("new", (phi.value(),)),
        Err(refused) => Ok(refusal(ruby, refused)),
    }
}

pub fn statistics(ruby: &Ruby, statistics: Option<Statistics>) -> Result<Option<Value>, Error> {
    let Some(statistics) = statistics else {
        return Ok(None);
    };
    data(ruby, "Statistics")?
        .funcall(
            "new",
            (
                statistics.samples,
                statistics.mean_ns,
                statistics.std_dev_ns,
                nanoseconds(statistics.min),
                nanoseconds(statistics.max),
            ),
        )
        .map(Some)
}
