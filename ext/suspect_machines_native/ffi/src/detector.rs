//! `SuspectMachines::Detector`: one peer on its own.
use std::cell::RefCell;

use magnus::{Error, Ruby, Value, prelude::*, typed_data::Obj};
use suspect_machines::Detector as Core;

use crate::{Number, WINDOW, convert, nanoseconds};

#[magnus::wrap(class = "SuspectMachines::Detector", free_immediately, size)]
pub struct Detector {
    core: RefCell<Core<WINDOW>>,
}

impl Detector {
    pub fn new(ruby: &Ruby, config: Value) -> Result<Self, Error> {
        let config = convert::config(ruby, config)?;
        let core = Core::new(config).map_err(|error| convert::config_error(ruby, error))?;
        Ok(Self {
            core: RefCell::new(core),
        })
    }

    pub fn heartbeat(ruby: &Ruby, this: Obj<Self>, now: Number) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().heartbeat(now.duration());
        convert::beat(ruby, outcome)
    }

    pub fn suspicion(ruby: &Ruby, this: &Self, now: Number) -> Result<Value, Error> {
        let outcome = this.core.borrow().suspicion(now.duration());
        convert::suspicion(ruby, outcome)
    }

    pub fn statistics(ruby: &Ruby, this: &Self) -> Result<Option<Value>, Error> {
        let statistics = this.core.borrow().statistics();
        convert::statistics(ruby, statistics)
    }

    pub fn samples(&self) -> usize {
        self.core.borrow().samples()
    }

    pub fn last_heartbeat(&self) -> Option<u64> {
        self.core.borrow().last_heartbeat().map(nanoseconds)
    }

    pub fn config(ruby: &Ruby, this: &Self) -> Result<Value, Error> {
        convert::config_value(ruby, this.core.borrow().config())
    }

    pub fn reset(this: Obj<Self>) -> Result<(), Error> {
        this.check_frozen()?;
        this.core.borrow_mut().reset();
        Ok(())
    }

    pub const fn window() -> usize {
        WINDOW
    }
}
