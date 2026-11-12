//! Private native entry point.
use crate::{CAPACITY, Roster, WINDOW};
use magnus::{Error, Ruby, function, method, prelude::*};

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("SuspectMachines")?;
    let class = module.define_class("Roster", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.const_set("CAPACITY", CAPACITY)?;
    class.const_set("WINDOW", WINDOW)?;
    class.define_singleton_method("_new", function!(Roster::new, 4))?;
    class.define_method("heartbeat", method!(Roster::heartbeat, 3))?;
    class.define_method("suspicion", method!(Roster::suspicion, 2))?;
    class.define_method("_suspects", method!(Roster::suspects, 4))?;
    class.define_method("statistics", method!(Roster::statistics, 1))?;
    class.define_method("incarnation", method!(Roster::incarnation, 1))?;
    class.define_method("last_heartbeat_ns", method!(Roster::last_heartbeat_ns, 1))?;
    class.define_method("forget", method!(Roster::forget, 1))?;
    class.define_method("size", method!(Roster::size, 0))?;
    class.define_method("capacity", function!(Roster::capacity, 0))?;
    class.define_method("window", function!(Roster::window, 0))?;
    Ok(())
}
