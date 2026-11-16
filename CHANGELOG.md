# Changelog

## [0.5.3](https://github.com/seuros/suspect_machines/compare/suspect_machines/v0.5.2...suspect_machines/v0.5.3) (2026-11-16)


### Bug Fixes

* fix wrapped classes at load so building a value runs no Ruby ([8f0f448](https://github.com/seuros/suspect_machines/commit/8f0f448eeb86b38f9f3cc070a7f15204ae2593a9))

## [0.5.2](https://github.com/seuros/suspect_machines/compare/suspect_machines/v0.5.1...suspect_machines/v0.5.2) (2026-11-16)


### Bug Fixes

* revalidate Config#with_acceptable_pause through the core ([52ce530](https://github.com/seuros/suspect_machines/commit/52ce5309b1ffec9922b2e1ca3720f6090637bb96))

## [0.5.1](https://github.com/seuros/suspect_machines/compare/suspect_machines/v0.5.0...suspect_machines/v0.5.1) (2026-11-14)


### Bug Fixes

* **deps:** require matryona-machines &gt;= 0.4 and read distributions through a names_only table ([2271642](https://github.com/seuros/suspect_machines/commit/2271642bd00feeaaaf43aee6e2c045ef283f1dc1))

## [0.5.0](https://github.com/seuros/suspect_machines/compare/suspect_machines/v0.4.0...suspect_machines/v0.5.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* adopt matryona 0.3 finite levels and integer_in scan limit

### Features

* adopt matryona 0.3 finite levels and integer_in scan limit ([0fb9975](https://github.com/seuros/suspect_machines/commit/0fb99759bf583910441742d8237954bed50213f2))

## [0.4.0](https://github.com/seuros/suspect_machines/compare/suspect_machines/v0.3.0...suspect_machines/v0.4.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* adopt matryona-machines 0.2: exact readers, symbols! tables, struct-only field reads

### Features

* adopt matryona-machines 0.2: exact readers, symbols! tables, struct-only field reads ([180ab6b](https://github.com/seuros/suspect_machines/commit/180ab6bd465bf6b05cf076c732d036a0ef26045c))

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
