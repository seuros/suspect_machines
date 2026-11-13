//! `SuspectMachines::Roster`.
use std::cell::RefCell;

use magnus::{Error, Ruby, Symbol, Value, prelude::*, typed_data::Obj};
use suspect_machines::{Forgetting, Roster as Core, Suspect};

use crate::{CAPACITY, Level, Number, WINDOW, convert, data, nanoseconds, refusal};

#[magnus::wrap(class = "SuspectMachines::Roster", free_immediately, size)]
pub struct Roster {
    core: RefCell<Box<Core<CAPACITY, WINDOW>>>,
}

impl Roster {
    pub fn new(ruby: &Ruby, config: Value) -> Result<Self, Error> {
        let config = convert::config(ruby, config)?;
        let core = Core::new(config).map_err(|error| convert::config_error(ruby, error))?;
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
        this.check_frozen()?;
        let outcome = this
            .core
            .borrow_mut()
            .heartbeat(peer.0, incarnation.0, now.duration());
        convert::beat(ruby, outcome)
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
        threshold: Level,
        cursor: Number,
        limit: Number,
    ) -> Result<Value, Error> {
        let mut buffer = [Suspect::default(); CAPACITY];
        let out = buffer.get_mut(..limit.index(ruby)?).ok_or_else(|| {
            Error::new(
                ruby.exception_range_error(),
                "limit exceeds the compiled capacity",
            )
        })?;
        let outcome =
            this.core
                .borrow()
                .scan(now.duration(), threshold.0, cursor.index(ruby)?, out);
        let scan = match outcome {
            Ok(scan) => scan,
            Err(refused) => return Ok(refusal(ruby, refused)),
        };
        let suspect = data(ruby, "Suspect")?;
        let suspects = ruby.ary_new_capa(scan.written);
        for found in out.iter().take(scan.written) {
            let entry: Value =
                suspect.funcall("new", (found.peer, found.incarnation, found.phi.value()))?;
            suspects.push(entry)?;
        }
        suspects.freeze();
        data(ruby, "Scan")?.funcall("new", (suspects, scan.next))
    }

    pub fn statistics(ruby: &Ruby, this: &Self, peer: Number) -> Result<Option<Value>, Error> {
        let statistics = this.core.borrow().statistics(peer.0);
        convert::statistics(ruby, statistics)
    }

    pub fn incarnation(&self, peer: Number) -> Option<u64> {
        self.core.borrow().incarnation(peer.0)
    }

    pub fn last_heartbeat(&self, peer: Number) -> Option<u64> {
        self.core.borrow().last_heartbeat(peer.0).map(nanoseconds)
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
        convert::config_value(ruby, this.core.borrow().config())
    }

    pub const fn capacity() -> usize {
        CAPACITY
    }

    pub const fn window() -> usize {
        WINDOW
    }
}
