//! `SuspectMachines::Roster`.
use std::cell::RefCell;

use magnus::{Error, Integer, Ruby, Symbol, Value, prelude::*, typed_data::Obj};
use matryona_machines::ruby::{Symbols, duration, integer, integer_in, keep, nanoseconds};
use suspect_machines::{Forgetting, Roster as Core, Suspect};

use crate::classes::classes;
use crate::{CAPACITY, WINDOW, convert, phi_value, refusal, threshold_from};

matryona_machines::symbols! {
    /// What `forget` answers.
    Forgettings for Forgetting as "a forgetting" {
        Forgotten => "forgotten",
        NotTracked => "not_tracked",
    }
}

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
        peer: Value,
        incarnation: Value,
        now: Value,
    ) -> Result<Value, Error> {
        this.check_frozen()?;
        let (peer, incarnation) = (integer(peer, "peer")?, integer(incarnation, "incarnation")?);
        let now = duration(now, "now")?;
        keep(
            ruby,
            this,
            &this.core,
            |core| core.heartbeat(peer, incarnation, now),
            |outcome| convert::beat(ruby, outcome),
        )
    }

    pub fn suspicion(ruby: &Ruby, this: &Self, peer: Value, now: Value) -> Result<Value, Error> {
        let (peer, now) = (integer(peer, "peer")?, duration(now, "now")?);
        let outcome = this.core.borrow().suspicion(peer, now);
        convert::suspicion(ruby, outcome)
    }

    /// `limit` is the length of the core's `out` buffer.
    pub fn scan(
        ruby: &Ruby,
        this: &Self,
        now: Value,
        threshold: Value,
        cursor: Value,
        limit: Value,
    ) -> Result<Value, Error> {
        let now = duration(now, "now")?;
        let cursor: usize = integer(cursor, "cursor")?;
        let limit = integer_in(limit, "limit", 0..=CAPACITY)?;
        let threshold = threshold_from(ruby, threshold)?;
        let classes = classes(ruby)?;
        let (suspect, scan_class) = (classes.suspect.get(ruby), classes.scan.get(ruby));
        let mut buffer = [Suspect::default(); CAPACITY];
        let out = buffer.get_mut(..limit).unwrap_or(&mut []);
        let outcome = this.core.borrow().scan(now, threshold, cursor, out);
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

    pub fn statistics(ruby: &Ruby, this: &Self, peer: Value) -> Result<Option<Value>, Error> {
        let statistics = this.core.borrow().statistics(integer(peer, "peer")?);
        convert::statistics(ruby, statistics)
    }

    pub fn incarnation(&self, peer: Value) -> Result<Option<u64>, Error> {
        Ok(self.core.borrow().incarnation(integer(peer, "peer")?))
    }

    pub fn last_heartbeat(ruby: &Ruby, this: &Self, peer: Value) -> Result<Option<Integer>, Error> {
        let last = this.core.borrow().last_heartbeat(integer(peer, "peer")?);
        Ok(last.map(|at| nanoseconds(ruby, at)))
    }

    pub fn forget(ruby: &Ruby, this: Obj<Self>, peer: Value) -> Result<Symbol, Error> {
        this.check_frozen()?;
        let peer = integer(peer, "peer")?;
        let outcome = this.core.borrow_mut().forget(peer);
        Ok(Forgettings::symbol(ruby, outcome))
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
