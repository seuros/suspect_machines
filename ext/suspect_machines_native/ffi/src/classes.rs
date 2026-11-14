//! The `Data` classes, resolved once at load and pinned. The exact-class
//! reads and clone-then-keep are `matryona_machines::ruby`'s.
//!
//! A value read back must be an instance of exactly the gem's class: a
//! subclass constant leaks past the seal and `Data#initialize` can be bound
//! directly, so `is_a?` is not enough. Its fields are read straight from the
//! struct (no reader runs) and rebuilt through the core constructor, which
//! `Marshal.load` skipped.
use magnus::{Error, RModule, Ruby};
use matryona_machines::ruby::{Class, Registry};

/// Every `Data` class the binding checks or builds.
pub struct Classes {
    pub phi: Class,
    pub threshold: Class,
    pub normal: Class,
    pub config: Class,
    pub recorded: Class,
    pub warming_up: Class,
    pub measured: Class,
    pub statistics: Class,
    pub suspect: Class,
    pub scan: Class,
}

static CLASSES: Registry<Classes> = Registry::new("suspect_machines");

/// Resolve the classes `suspect_machines.rb` defined before requiring the
/// extension.
pub fn resolve(ruby: &Ruby, module: RModule) -> Result<&'static Classes, Error> {
    let class = |name| Class::resolve(ruby, module, name);
    let resolved = Classes {
        phi: class("Phi")?,
        threshold: class("Threshold")?,
        normal: class("Normal")?,
        config: class("Config")?,
        recorded: class("Recorded")?,
        warming_up: class("WarmingUp")?,
        measured: class("Measured")?,
        statistics: class("Statistics")?,
        suspect: class("Suspect")?,
        scan: class("Scan")?,
    };
    CLASSES.set(ruby, resolved)
}

/// The classes resolved at load.
pub fn classes(ruby: &Ruby) -> Result<&'static Classes, Error> {
    CLASSES.get(ruby)
}
