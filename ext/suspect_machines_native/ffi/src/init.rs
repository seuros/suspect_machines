//! Private native entry point.
use magnus::{Error, Ruby, Value, function, method, prelude::*};

use crate::{CAPACITY, Number, WINDOW, convert, data, detector::Detector, roster::Roster};

/// `Normal#__validate`: the floor is `u64` nanoseconds. Whether it is usable
/// is `Config`'s to say, as in the core.
fn validate_normal(this: Value) -> Result<(), Error> {
    this.funcall::<_, _, Number>("min_std_dev", ()).map(|_| ())
}

fn validate_config(ruby: &Ruby, this: Value) -> Result<(), Error> {
    convert::config(ruby, this).map(|_| ())
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("SuspectMachines")?;
    data(ruby, "Normal")?.define_private_method("__validate", method!(validate_normal, 0))?;
    data(ruby, "Config")?.define_private_method("__validate", method!(validate_config, 0))?;

    let class = module.define_class("Detector", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.const_set("WINDOW", WINDOW)?;
    class.define_singleton_method("new", function!(Detector::new, 1))?;
    class.define_method("heartbeat", method!(Detector::heartbeat, 1))?;
    class.define_method("suspicion", method!(Detector::suspicion, 1))?;
    class.define_method("statistics", method!(Detector::statistics, 0))?;
    class.define_method("samples", method!(Detector::samples, 0))?;
    class.define_method("last_heartbeat", method!(Detector::last_heartbeat, 0))?;
    class.define_method("config", method!(Detector::config, 0))?;
    class.define_method("reset", method!(Detector::reset, 0))?;
    class.define_method("window", function!(Detector::window, 0))?;

    let class = module.define_class("Roster", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.const_set("CAPACITY", CAPACITY)?;
    class.const_set("WINDOW", WINDOW)?;
    class.define_singleton_method("new", function!(Roster::new, 1))?;
    class.define_method("heartbeat", method!(Roster::heartbeat, 3))?;
    class.define_method("suspicion", method!(Roster::suspicion, 2))?;
    class.define_method("scan", method!(Roster::scan, 4))?;
    class.define_method("statistics", method!(Roster::statistics, 1))?;
    class.define_method("incarnation", method!(Roster::incarnation, 1))?;
    class.define_method("last_heartbeat", method!(Roster::last_heartbeat, 1))?;
    class.define_method("forget", method!(Roster::forget, 1))?;
    class.define_method("size", method!(Roster::size, 0))?;
    class.define_method("empty?", method!(Roster::is_empty, 0))?;
    class.define_method("config", method!(Roster::config, 0))?;
    class.define_method("capacity", function!(Roster::capacity, 0))?;
    class.define_method("window", function!(Roster::window, 0))?;
    Ok(())
}
