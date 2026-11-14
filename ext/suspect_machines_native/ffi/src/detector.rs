//! `SuspectMachines::Detector`: one peer on its own.
use std::cell::RefCell;

use magnus::{Error, Ruby, Value, prelude::*, typed_data::Obj};
use suspect_machines::Detector as Core;

use crate::classes::keep;
use crate::{Number, WINDOW, convert, nanoseconds_or_nil};

#[magnus::wrap(class = "SuspectMachines::Detector", free_immediately, size)]
pub struct Detector {
    core: RefCell<Core<WINDOW>>,
}

impl Detector {
    pub fn new(ruby: &Ruby, config: Value) -> Result<Self, Error> {
        let core = convert::configured(ruby, config, Core::new)?;
        Ok(Self {
            core: RefCell::new(core),
        })
    }

    /// Clone-then-keep: a `Recorded` answer is built through `new`.
    pub fn heartbeat(ruby: &Ruby, this: Obj<Self>, now: Number) -> Result<Value, Error> {
        keep(
            ruby,
            this,
            &this.core,
            |core| core.heartbeat(now.duration()),
            |outcome| convert::beat(ruby, outcome),
        )
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

    pub fn last_heartbeat(ruby: &Ruby, this: &Self) -> Result<Option<u64>, Error> {
        nanoseconds_or_nil(ruby, this.core.borrow().last_heartbeat())
    }

    pub fn config(ruby: &Ruby, this: &Self) -> Result<Value, Error> {
        let config = this.core.borrow().config();
        convert::config_value(ruby, config)
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
