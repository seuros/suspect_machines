# Changelog

## [0.3.0](https://github.com/seuros/suspect_machines/compare/suspect_machines/v0.2.0...suspect_machines/v0.3.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* core-validated sealed values, private Normal/Statistics/Suspect/Scan fields, Scan#suspects, clone-then-keep heartbeat

### Features

* core-validated sealed values, private Normal/Statistics/Suspect/Scan fields, Scan#suspects, clone-then-keep heartbeat ([e155c5f](https://github.com/seuros/suspect_machines/commit/e155c5f59aa257252775378d9763bd676a669ed9))

## [0.2.0](https://github.com/seuros/suspect_machines/compare/suspect_machines/v0.1.1...suspect_machines/v0.2.0) (2026-11-13)


### ⚠ BREAKING CHANGES

* see CONVENTIONS: one exhaustive Refusal per crate, Result from every mutator, honoured idempotent repeats, Ruby mirrors the core. No compatibility aliases. Refusal::Duplicate is renamed SameInstant; Roster::forget returns Forgetting.

### Features

* Forgetting outcome, SameInstant refusal, Ruby Config, Detector and scan mirror the core ([c936cbe](https://github.com/seuros/suspect_machines/commit/c936cbe6840e915099cacf4589824c9e829d784c))

## [0.1.1](https://github.com/seuros/suspect_machines/compare/suspect_machines-v0.1.0...suspect_machines/v0.1.1) (2026-11-12)


### Features

* initial extraction ([9ae1485](https://github.com/seuros/suspect_machines/commit/9ae1485a042ea92dbdb26c143f19c40797247327))
