//! Thin binding to the bounded phi-accrual roster.
mod init;

use std::{cell::RefCell, time::Duration};

use magnus::{Error, Float, Integer, Ruby, Symbol, TryConvert, Value, prelude::*, typed_data::Obj};
use suspect_machines::{
    Beat, Config, ConfigError, Distribution, Refusal, Roster as Core, Suspect, Suspicion, Threshold,
};

const CAPACITY: usize = 64;
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
}

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

const fn refusal(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::TimeRegression => "time_regression",
        Refusal::Duplicate => "duplicate",
        Refusal::Stale => "stale",
        Refusal::Full => "full",
        Refusal::Untracked => "untracked",
        _ => "rejected",
    }
}

const fn config_error(error: ConfigError) -> &'static str {
    match error {
        ConfigError::ZeroWarmUp => "min_samples must be at least 1",
        ConfigError::ZeroDeviationFloor => "min_std_dev_ns must be at least 1",
        ConfigError::WarmUpExceedsWindow => "min_samples exceeds the compiled window",
        _ => "configuration out of range",
    }
}

fn nanoseconds(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

#[magnus::wrap(class = "SuspectMachines::Roster", free_immediately, size)]
struct Roster {
    core: RefCell<Box<Core<CAPACITY, WINDOW>>>,
}

impl Roster {
    fn new(
        ruby: &Ruby,
        distribution: Symbol,
        min_samples: Number,
        min_std_dev_ns: Number,
        acceptable_pause_ns: Number,
    ) -> Result<Self, Error> {
        let distribution = match &*distribution.name()? {
            "normal" => Distribution::Normal {
                min_std_dev: Duration::from_nanos(min_std_dev_ns.0),
            },
            "exponential" => Distribution::Exponential,
            _ => {
                return Err(Error::new(
                    ruby.exception_arg_error(),
                    "distribution must be :normal or :exponential",
                ));
            }
        };
        let out_of_range = |error| Error::new(ruby.exception_range_error(), config_error(error));
        let config = Config::new(distribution, min_samples.index(ruby)?)
            .and_then(|config| {
                config.with_acceptable_pause(Duration::from_nanos(acceptable_pause_ns.0))
            })
            .map_err(out_of_range)?;
        let core = Core::new(config).map_err(out_of_range)?;
        Ok(Self {
            core: RefCell::new(Box::new(core)),
        })
    }

    fn heartbeat(
        ruby: &Ruby,
        this: Obj<Self>,
        peer: Number,
        incarnation: Number,
        now_ns: Number,
    ) -> Result<Symbol, Error> {
        this.check_frozen()?;
        let outcome =
            this.core
                .borrow_mut()
                .heartbeat(peer.0, incarnation.0, Duration::from_nanos(now_ns.0));
        Ok(ruby.to_symbol(match outcome {
            Ok(Beat::First) => "first",
            Ok(Beat::Restarted) => "restarted",
            Ok(_) => "recorded",
            Err(error) => refusal(error),
        }))
    }

    fn suspicion(ruby: &Ruby, this: &Self, peer: Number, now_ns: Number) -> Value {
        let outcome = this
            .core
            .borrow()
            .suspicion(peer.0, Duration::from_nanos(now_ns.0));
        match outcome {
            Ok(Suspicion::Measured(phi)) => ruby.float_from_f64(phi.value()).as_value(),
            Ok(_) => ruby.to_symbol("warming_up").as_value(),
            Err(error) => ruby.to_symbol(refusal(error)).as_value(),
        }
    }

    fn suspects(
        ruby: &Ruby,
        this: &Self,
        now_ns: Number,
        threshold: Level,
        cursor: Number,
        limit: Number,
    ) -> Result<Value, Error> {
        let mut buffer = [Suspect::default(); CAPACITY];
        let page = buffer.get_mut(..limit.index(ruby)?).ok_or_else(|| {
            Error::new(
                ruby.exception_range_error(),
                "limit exceeds the compiled capacity",
            )
        })?;
        let scan = this
            .core
            .borrow()
            .scan(
                Duration::from_nanos(now_ns.0),
                threshold.0,
                cursor.index(ruby)?,
                page,
            )
            .map_err(|error| Error::new(ruby.exception_range_error(), refusal(error)))?;
        let module = ruby.define_module("SuspectMachines")?;
        let suspect: Value = module.const_get("Suspect")?;
        let suspects = ruby.ary_new_capa(scan.written);
        for found in page.iter().take(scan.written) {
            let entry: Value =
                suspect.funcall("new", (found.peer, found.incarnation, found.phi.value()))?;
            suspects.push(entry)?;
        }
        let result: Value = module.const_get("Scan")?;
        result.funcall("new", (suspects, scan.next))
    }

    fn statistics(ruby: &Ruby, this: &Self, peer: Number) -> Result<Value, Error> {
        let Some(statistics) = this.core.borrow().statistics(peer.0) else {
            return Ok(ruby.qnil().as_value());
        };
        let class: Value = ruby
            .define_module("SuspectMachines")?
            .const_get("Statistics")?;
        class.funcall(
            "new",
            (
                statistics.samples,
                statistics.mean_ns,
                statistics.std_dev_ns,
                nanoseconds(statistics.min),
                nanoseconds(statistics.max),
            ),
        )
    }

    fn incarnation(&self, peer: Number) -> Option<u64> {
        self.core.borrow().incarnation(peer.0)
    }

    fn last_heartbeat_ns(&self, peer: Number) -> Option<u64> {
        self.core.borrow().last_heartbeat(peer.0).map(nanoseconds)
    }

    fn forget(this: Obj<Self>, peer: Number) -> Result<bool, Error> {
        this.check_frozen()?;
        Ok(this.core.borrow_mut().forget(peer.0))
    }

    fn size(&self) -> usize {
        self.core.borrow().len()
    }

    const fn capacity() -> usize {
        CAPACITY
    }

    const fn window() -> usize {
        WINDOW
    }
}
