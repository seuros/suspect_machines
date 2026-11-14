//! Private native entry point.
use magnus::{Error, Ruby, function, method, prelude::*};

use crate::{CAPACITY, WINDOW, classes, detector::Detector, roster::Roster, values};

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("SuspectMachines")?;
    module.define_singleton_method("__phi", function!(values::phi_level, 1))?;
    module.define_singleton_method("__threshold", function!(values::threshold_level, 1))?;
    module.define_singleton_method("__normal", function!(values::normal, 1))?;
    module.define_singleton_method("__config", function!(values::config, 3))?;
    module.define_singleton_method("__recorded", function!(values::recorded, 2))?;
    module.define_singleton_method("__warming_up", function!(values::warming_up, 3))?;
    module.define_singleton_method("__measured", function!(values::measured, 1))?;
    module.define_singleton_method("__statistics", function!(values::statistics, 5))?;
    module.define_singleton_method("__suspect", function!(values::suspect, 3))?;
    module.define_singleton_method("__scan", function!(values::scan, 2))?;
    let phi = classes::resolve(ruby, module)?.phi.get(ruby);
    phi.define_method("millis", method!(values::millis, 0))?;
    phi.define_method("reaches?", method!(values::reaches, 1))?;

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
