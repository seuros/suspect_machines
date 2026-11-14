//! Every value the core answers has a validating constructor, and what the
//! detector and roster answer always passes it.
use super::*;
use crate::test_support::{Rng, at, detector, exponential, normal, roster, threshold};

#[test]
fn phi_and_threshold_hold_only_finite_levels() {
    assert_eq!(Phi::new(0.0).map(Phi::value), Some(0.0));
    assert!(Phi::new(-0.0).is_some_and(|phi| phi.value().is_sign_positive()));
    assert_eq!(Phi::new(2.5).map(Phi::millis), Some(2_500));
    assert_eq!(Phi::new(1e300).map(Phi::millis), Some(u64::MAX));
    for bad in [-1e-300, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(Phi::new(bad), None, "{bad}");
    }
    let level = Phi::new(3.0).expect("a finite level");
    assert!(level.reaches(threshold(3.0)));
    assert!(!level.reaches(threshold(3.000_001)));
    assert_eq!(Phi::default(), Phi::new(0.0).expect("zero"));
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(Threshold::new(bad), None, "{bad}");
    }
    assert_eq!(threshold(8.0).value(), 8.0);
}

#[test]
fn answer_constructors_hold_what_a_window_and_a_roster_can_answer() {
    let (one, two) = (at(1), at(2));
    let recorded = Beat::recorded::<4>(one, 4);
    assert_eq!(
        recorded,
        Some(Beat::Recorded {
            interval: one,
            samples: 4
        })
    );
    let warming = [(0, 1, at(9)), (3, 4, Duration::ZERO)];
    let statistics = [(4, 1.5, 0.5, one, two)];
    let scans = [(4, None), (0, Some(3))];
    for (samples, required, silence) in warming {
        assert!(Suspicion::warming_up::<4>(samples, required, silence).is_some());
    }
    for (samples, mean, spread, min, max) in statistics {
        let built = Statistics::new::<4>(samples, mean, spread, min, max).expect("valid");
        assert_eq!(
            (built.samples(), built.mean_ns(), built.std_dev_ns()),
            (samples, mean, spread)
        );
        assert_eq!((built.min(), built.max()), (min, max));
    }
    for (suspects, next) in scans {
        let scan = Scan::new::<4>(suspects, next).expect("valid");
        assert_eq!((scan.suspects(), scan.next()), (suspects, next));
    }
    for (interval, samples) in [(Duration::ZERO, 1), (one, 0), (one, 5)] {
        assert_eq!(Beat::recorded::<4>(interval, samples), None);
    }
    for (samples, required) in [(1, 1), (0, 0), (0, 5)] {
        assert_eq!(Suspicion::warming_up::<4>(samples, required, at(9)), None);
    }
    for (samples, mean, spread, min, max) in [
        (0, 1.0, 0.0, one, one),
        (5, 1.0, 0.0, one, one),
        (1, f64::NAN, 0.0, one, one),
        (1, -1.0, 0.0, one, one),
        (1, 1.0, f64::INFINITY, one, one),
        (1, 1.0, 0.0, Duration::ZERO, one),
        (1, 1.0, 0.0, two, one),
        (2, 0.5, 0.0, one, two),  // a mean below the shortest interval
        (2, 2.5, 0.0, one, two),  // a mean above the longest
        (1, -0.0, 0.0, one, one), // a negative zero mean
        (1, 1.0, -0.0, one, one), // a negative zero deviation
        (2, 1.5, 0.6, one, two),  // a deviation above half the range
        (1, 1.5, 0.5, one, two),  // one interval is its own min and max
    ] {
        assert_eq!(Statistics::new::<4>(samples, mean, spread, min, max), None);
    }
    for (suspects, next) in [(5, None), (0, Some(4))] {
        assert_eq!(Scan::new::<4>(suspects, next), None);
    }
    let suspect = Suspect::new(u64::MAX, 0, Phi::new(4.0).expect("finite"));
    assert_eq!(
        (suspect.peer(), suspect.incarnation(), suspect.phi().value()),
        (u64::MAX, 0, 4.0)
    );
}

/// Every answer of a detector and a roster over random schedules is a value
/// its constructor accepts, so a value rebuilt from its fields is the same.
#[test]
fn answered_values_pass_their_constructors() {
    let mut rng = Rng::new(0x5EED);
    let mut single = detector::<8>(normal(1, 3));
    let mut table = roster::<4, 8>(exponential(2));
    let mut now = 0;
    for _ in 0..2_000 {
        now += 1 + rng.next() % 50;
        let peer = rng.next() % 6;
        for beat in [single.heartbeat(at(now)), table.heartbeat(peer, 0, at(now))] {
            if let Ok(Beat::Recorded { interval, samples }) = beat {
                assert!(Beat::recorded::<8>(interval, samples).is_some());
            }
        }
        for suspicion in [
            single.suspicion(at(now + 7)),
            table.suspicion(peer, at(now + 7)),
        ] {
            match suspicion {
                Ok(Suspicion::WarmingUp {
                    samples,
                    required,
                    silence,
                }) => assert!(Suspicion::warming_up::<8>(samples, required, silence).is_some()),
                Ok(Suspicion::Measured(phi)) => assert_eq!(Phi::new(phi.value()), Some(phi)),
                Ok(Suspicion::NoHeartbeat) => {}
                Err(refusal) => assert_eq!(refusal, Refusal::Untracked),
            }
        }
        for stats in [single.statistics(), table.statistics(peer)]
            .into_iter()
            .flatten()
        {
            let rebuilt = Statistics::new::<8>(
                stats.samples(),
                stats.mean_ns(),
                stats.std_dev_ns(),
                stats.min(),
                stats.max(),
            );
            assert_eq!(rebuilt, Some(stats));
        }
        let mut out = [Suspect::default(); 2];
        let cursor = usize::try_from(rng.next() % 4).expect("below four");
        let scan = table
            .scan(at(now + 7), threshold(0.5), cursor, &mut out)
            .expect("an instant that fits u64 nanoseconds");
        assert_eq!(Scan::new::<4>(scan.suspects(), scan.next()), Some(scan));
        if rng.next().is_multiple_of(40) {
            let tracked = table.incarnation(peer).is_some();
            assert_eq!(table.forget(peer) == Forgetting::Forgotten, tracked);
        }
    }
}

/// A restart builds the new incarnation's window directly: the roster ends
/// exactly where a fresh roster fed the same heartbeat would.
#[test]
fn restart_equals_a_fresh_first_heartbeat() {
    let mut table = roster::<2, 4>(exponential(1));
    for now in [10, 20, 35] {
        assert!(table.heartbeat(7, 1, at(now)).is_ok());
    }
    assert_eq!(table.heartbeat(7, 2, at(40)), Ok(Beat::Restarted));
    let mut fresh = roster::<2, 4>(exponential(1));
    assert_eq!(fresh.heartbeat(7, 2, at(40)), Ok(Beat::First));
    assert_eq!(table, fresh);
}
