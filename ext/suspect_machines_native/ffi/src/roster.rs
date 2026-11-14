//! `SuspectMachines::Roster`.
use std::cell::RefCell;

use magnus::{Error, Ruby, Symbol, Value, prelude::*, typed_data::Obj};
use suspect_machines::{Forgetting, Roster as Core, Suspect};

use crate::classes::{classes, keep};
use crate::{
    CAPACITY, Number, WINDOW, convert, nanoseconds_or_nil, phi_value, refusal, threshold_from,
};

#[magnus::wrap(class = "SuspectMachines::Roster", free_immediately, size)]
pub struct Roster {
    core: RefCell<Box<Core<CAPACITY, WINDOW>>>,
}

impl Roster {
    pub fn new(ruby: &Ruby, config: Value) -> Result<Self, Error> {
        let core = convert::configured(ruby, config, Core::new)?;
        Ok(Self {
            core: RefCell::new(Box::new(core)),
        })
    }

    pub fn heartbeat(
        ruby: &Ruby,
        this: Obj<Self>,
        peer: Number,
        incarnation: Number,
        now: Number,
    ) -> Result<Value, Error> {
        keep(
            ruby,
            this,
            &this.core,
            |core| core.heartbeat(peer.0, incarnation.0, now.duration()),
            |outcome| convert::beat(ruby, outcome),
        )
    }

    pub fn suspicion(ruby: &Ruby, this: &Self, peer: Number, now: Number) -> Result<Value, Error> {
        let outcome = this.core.borrow().suspicion(peer.0, now.duration());
        convert::suspicion(ruby, outcome)
    }

    /// `limit` is the length of the core's `out` buffer.
    pub fn scan(
        ruby: &Ruby,
        this: &Self,
        now: Number,
        threshold: Value,
        cursor: Number,
        limit: Number,
    ) -> Result<Value, Error> {
        let (threshold, cursor) = (threshold_from(ruby, threshold)?, cursor.index(ruby)?);
        let classes = classes(ruby)?;
        let (suspect, scan_class) = (
            ruby.get_inner(classes.suspect),
            ruby.get_inner(classes.scan),
        );
        let mut buffer = [Suspect::default(); CAPACITY];
        let out = buffer.get_mut(..limit.index(ruby)?).ok_or_else(|| {
            Error::new(
                ruby.exception_range_error(),
                "limit exceeds the compiled capacity",
            )
        })?;
        let outcome = this
            .core
            .borrow()
            .scan(now.duration(), threshold, cursor, out);
        let scan = match outcome {
            Ok(scan) => scan,
            Err(refused) => return Ok(refusal(ruby, refused)),
        };
        let suspects = ruby.ary_new_capa(scan.suspects());
        for found in out.iter().take(scan.suspects()) {
            let phi = phi_value(ruby, found.phi())?;
            let entry: Value = suspect.funcall("new", (found.peer(), found.incarnation(), phi))?;
            suspects.push(entry)?;
        }
        scan_class.funcall("new", (suspects, scan.next()))
    }

    pub fn statistics(ruby: &Ruby, this: &Self, peer: Number) -> Result<Option<Value>, Error> {
        let statistics = this.core.borrow().statistics(peer.0);
        convert::statistics(ruby, statistics)
    }

    pub fn incarnation(&self, peer: Number) -> Option<u64> {
        self.core.borrow().incarnation(peer.0)
    }

    pub fn last_heartbeat(ruby: &Ruby, this: &Self, peer: Number) -> Result<Option<u64>, Error> {
        nanoseconds_or_nil(ruby, this.core.borrow().last_heartbeat(peer.0))
    }

    pub fn forget(ruby: &Ruby, this: Obj<Self>, peer: Number) -> Result<Symbol, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().forget(peer.0);
        Ok(ruby.to_symbol(match outcome {
            Forgetting::Forgotten => "forgotten",
            Forgetting::NotTracked => "not_tracked",
        }))
    }

    pub fn size(&self) -> usize {
        self.core.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.core.borrow().is_empty()
    }

    pub fn config(ruby: &Ruby, this: &Self) -> Result<Value, Error> {
        let config = this.core.borrow().config();
        convert::config_value(ruby, config)
    }

    pub const fn capacity() -> usize {
        CAPACITY
    }

    pub const fn window() -> usize {
        WINDOW
    }
}
