# suspect-machines

> **AMS Hayashibara**, NCC-4223, *Hayashibara*-class picket vessel.

Engine-room flight manual: turn heartbeat arrival times into a suspicion level,
and nothing more than a suspicion level.

Allocation-free `no_std` phi-accrual failure detector (Hayashibara et al.,
2004). A `Detector<W>` keeps the last `W` heartbeat intervals of one peer and
their mean and population standard deviation. `Roster<P, W>` keeps up to `P`
such windows keyed by a caller-assigned `u64` peer id and an incarnation
number. Asked at an explicit instant, either one answers with
`phi = -log10(P(a heartbeat still arrives after this much silence))`.

Everything the detector assumes is stated by the caller:

- **Distribution.** `Distribution::Normal(Normal::new(min_std_dev)?)` uses the
  window mean and standard deviation, floored at `min_std_dev` (`Normal::new`
  refuses a zero floor with `ConfigError::ZeroDeviationFloor`).
  `Distribution::Exponential` uses only the mean. Neither is verified against
  the data.
- **Warm-up.** `Config::new(distribution, min_samples)`: until `min_samples`
  intervals are in the window, the answer is `Suspicion::WarmingUp` with the
  current silence, never a phi computed from too little data.
- **Acceptable pause.** `with_acceptable_pause` tolerates a fixed extra
  silence before suspicion starts to accrue.

Time is explicit: the caller passes a monotonic `Duration` to every call. A
heartbeat earlier than the previous one is `Refusal::TimeRegression`, an equal
one is `Refusal::SameInstant`, a time beyond `u64` nanoseconds is
`Refusal::Overflow`. A refused call changes nothing. A full window drops its
oldest interval; a full roster answers `Refusal::Full` and never evicts a peer.
A heartbeat with an older incarnation is `Refusal::Stale`; a newer one discards
the peer's statistics and starts a new warm-up. `Roster::scan` writes peers at
or above a threshold into a caller buffer and returns a cursor when it runs out
of room; a peer whose last heartbeat is later than the scan instant has no
silence to measure and is skipped, not refused (`Roster::suspicion` refuses it
with `TimeRegression`). `Roster::forget` never refuses: it answers `Forgetting::Forgotten`, or
`Forgetting::NotTracked` for the idempotent repeat that changed nothing.

Every value has a validating constructor, its fields are private behind
readers, and every answer passes it: `Normal::new` (a floor of `1 ns` to
`u64::MAX` ns), `Phi::new` (finite, not negative), `Threshold::new` (finite,
above zero),
`Beat::recorded::<W>` (a non-zero interval, `1..=W` samples),
`Suspicion::warming_up::<W>` (`samples < required <= W`),
`Statistics::new::<W>` (`1..=W` samples, `1 ns <= min <= max` with one
sample its own min and max, a mean within `min..=max`, a positively signed
deviation of at most `(max - min) / 2`), `Suspect::new` (any peer and incarnation)
and `Scan::new::<P>` (at most `P` suspects, a resume cursor below `P`). Each
answers `None` (or the `ConfigError`) for anything else.

Clock: every honoured heartbeat (`First`, `Recorded`, `Restarted`) records
`now` as the last heartbeat; it is the observation itself, and there is no
honoured repeat. `suspicion` and `scan` take `&self` and record nothing.

The Ruby gem passes and answers every `Duration` as Integer nanoseconds and
mirrors `Phi`, `Threshold`, `Normal`, `Config`, `Beat::Recorded`,
`Suspicion::WarmingUp`, `Suspicion::Measured`, `Statistics`, `Suspect` and
`Scan` as `Data` values with the same fields (its `Scan#suspects` holds the
suspects themselves where `Scan::suspects` counts those written to `out`),
each checked through the constructors above
when built. Unit variants are symbols.

Phi is advisory. It describes arrival statistics under an assumed
distribution. It is not a verdict that a peer is dead, it does not revoke a
lease or an election, and it cannot stand in for durable authority or
resource-side fencing.

```rust
use core::time::Duration;
use suspect_machines::{Beat, Config, Detector, Distribution, Normal, Suspicion, Threshold};

let normal = Normal::new(Duration::from_millis(100)).expect("a non-zero floor");
let config = Config::new(Distribution::Normal(normal), 3).expect("valid policy");
let mut detector = Detector::<16>::new(config).expect("warm-up fits the window");

assert_eq!(detector.heartbeat(Duration::from_secs(0)), Ok(Beat::First));
for second in 1..=3 {
    detector.heartbeat(Duration::from_secs(second)).expect("monotonic");
}
// Regular 1 s beats, floor 100 ms: 1.3 s of silence is three deviations late.
let Ok(Suspicion::Measured(phi)) = detector.suspicion(Duration::from_millis(4_300)) else {
    panic!("warm-up is over");
};
assert!((phi.value() - 2.8697).abs() < 1e-4);
assert!(!phi.reaches(Threshold::new(8.0).expect("finite and positive")));
```

## Arithmetic

`f64::sqrt`, `exp` and `ln` live in `std`, so the crate carries its own, built
only from IEEE-754 add, subtract, multiply and divide in a fixed order with
fixed iteration counts. No `libm`, no fused multiply-add, no lookup of platform
behavior: the same calls give the same bits on every target.

The normal tail is evaluated in the log domain (a positive-term `erf` series
below `|z| = 2.83`, a 60-level `erfc` continued fraction above), so phi stays
finite and accurate long after the probability itself would underflow.

| routine | domain | bound against the exact value |
|:--------|:-------|:------------------------------|
| `sqrt` | finite `x >= 0` | relative `4e-16` |
| `exp` | `-708 <= x <= 709` | relative `1e-14` |
| `ln` | positive finite `x` | `1e-15` absolute or `1e-14` relative |
| normal phi from `z` | finite `z` | `1e-12` absolute or `1e-12` relative |

The tests check these against `std`, against 80-digit reference values and
against an independent quadrature of the Gaussian tail. The bounds cover the
function evaluation. They do not cover the inputs: intervals are converted to
`f64` nanoseconds, which is exact up to `2^53` ns (about 104 days) and rounded
to nearest beyond that.

Statistics are recomputed from the window on each read: `O(W)` per suspicion,
`O(P * W)` per full scan, with an exact integer sum behind the mean.
