#![allow(clippy::float_cmp, clippy::cast_precision_loss)]

use super::*;
use crate::test_support::{
    Expected, MILLI, Model, Rng, SECOND, at, close, detector, exponential, measured, normal,
    random_config, random_gap, threshold,
};

#[test]
fn first_heartbeat_warm_up_and_measurement_are_explicit() {
    let mut detector = detector::<4>(normal(100 * MILLI, 2));
    assert_eq!(detector.suspicion(at(5)), Ok(Suspicion::NoHeartbeat));
    assert_eq!(detector.statistics(), None);
    assert_eq!(detector.last_heartbeat(), None);
    assert_eq!(detector.heartbeat(at(SECOND)), Ok(Beat::First));
    assert_eq!(
        detector.suspicion(at(3 * SECOND)),
        Ok(Suspicion::WarmingUp {
            samples: 0,
            required: 2,
            silence: at(2 * SECOND),
        })
    );
    assert_eq!(
        detector.heartbeat(at(2 * SECOND)),
        Ok(Beat::Recorded {
            interval: at(SECOND),
            samples: 1,
        })
    );
    assert!(matches!(
        detector.suspicion(at(60 * SECOND)),
        Ok(Suspicion::WarmingUp { samples: 1, .. })
    ));
    assert!(detector.heartbeat(at(3 * SECOND)).is_ok());
    assert_eq!(detector.samples(), 2);
    assert_eq!(detector.last_heartbeat(), Some(at(3 * SECOND)));
    // Regular 1 s beats, floor 100 ms: 1.3 s of silence is z = 3.
    let phi = measured(detector.suspicion(at(4_300 * MILLI)));
    assert!(close(phi, 2.869_699_035_929_369, 1e-9));
    // On time is the median: Q = 1/2.
    let on_time = measured(detector.suspicion(at(4 * SECOND)));
    assert!(close(on_time, std::f64::consts::LOG10_2, 1e-9));
    detector.reset();
    assert_eq!(detector.suspicion(at(0)), Ok(Suspicion::NoHeartbeat));
    assert_eq!(detector.samples(), 0);
    assert_eq!(detector.config(), normal(100 * MILLI, 2));
    assert_eq!(Detector::<4>::WINDOW, 4);
}

#[test]
fn statistics_slide_over_the_last_window_intervals() {
    let mut detector = detector::<3>(exponential(1));
    let mut now = 0;
    assert_eq!(detector.heartbeat(at(now)), Ok(Beat::First));
    for gap in [10, 20, 30, 40, 100] {
        now += gap;
        assert!(detector.heartbeat(at(now)).is_ok());
    }
    // Window holds 30, 40, 100.
    let statistics = detector.statistics().unwrap_or_else(|| panic!("no data"));
    assert_eq!(statistics.samples, 3);
    assert!(close(statistics.mean_ns, 170.0 / 3.0, 1e-12));
    let variance = (30f64.powi(2) + 40f64.powi(2) + 100f64.powi(2)) / 3.0 - (170f64 / 3.0).powi(2);
    assert!(close(statistics.std_dev_ns, variance.sqrt(), 1e-12));
    assert_eq!(statistics.min, at(30));
    assert_eq!(statistics.max, at(100));
    // Exponential: phi = silence / (mean * ln 10).
    let phi = measured(detector.suspicion(at(now + 170)));
    assert!(close(phi, 3.0 * std::f64::consts::LOG10_E, 1e-12));
    assert_eq!(measured(detector.suspicion(at(now))), 0.0);
}

#[test]
fn acceptable_pause_shifts_both_distributions() {
    let pause = at(500 * MILLI);
    let shifted = |config: Config| {
        config
            .with_acceptable_pause(pause)
            .unwrap_or_else(|error| panic!("{error:?}"))
    };
    for (plain, padded) in [
        (normal(100 * MILLI, 1), shifted(normal(100 * MILLI, 1))),
        (exponential(1), shifted(exponential(1))),
    ] {
        assert_eq!(padded.acceptable_pause(), pause);
        let mut strict = detector::<4>(plain);
        let mut lenient = detector::<4>(padded);
        for beat in 0..4 {
            assert!(strict.heartbeat(at(beat * SECOND)).is_ok());
            assert!(lenient.heartbeat(at(beat * SECOND)).is_ok());
        }
        let late = measured(strict.suspicion(at(4_200 * MILLI)));
        let excused = measured(lenient.suspicion(at(4_700 * MILLI)));
        assert!(close(late, excused, 1e-9), "{late} vs {excused}");
        assert!(measured(lenient.suspicion(at(4_200 * MILLI))) < late);
    }
    let mut lenient = detector::<4>(shifted(exponential(1)));
    assert!(lenient.heartbeat(at(0)).is_ok());
    assert!(lenient.heartbeat(at(SECOND)).is_ok());
    assert_eq!(measured(lenient.suspicion(at(SECOND + 400 * MILLI))), 0.0);
}

#[test]
fn deviation_floor_bounds_regular_senders_and_real_jitter_overrides_it() {
    let mut regular = detector::<8>(normal(50 * MILLI, 1));
    let mut jittery = detector::<8>(normal(50 * MILLI, 1));
    let mut now = 0;
    assert!(regular.heartbeat(at(0)).is_ok());
    assert!(jittery.heartbeat(at(0)).is_ok());
    for beat in 1..=8u64 {
        assert!(regular.heartbeat(at(beat * SECOND)).is_ok());
        now += if beat % 2 == 0 {
            600 * MILLI
        } else {
            1_400 * MILLI
        };
        assert!(jittery.heartbeat(at(now)).is_ok());
    }
    let statistics = regular.statistics().unwrap_or_else(|| panic!("no data"));
    assert_eq!(statistics.std_dev_ns, 0.0);
    assert_eq!(statistics.mean_ns, SECOND as f64);
    // sigma = floor = 50 ms, so 150 ms late is z = 3, not infinity.
    let floored = measured(regular.suspicion(at(8 * SECOND + 1_150 * MILLI)));
    assert!(close(floored, 2.869_699_035_929_369, 1e-9));
    // sigma = 400 ms measured, above the floor: 1.2 s late is z = 3.
    let spread = jittery.statistics().unwrap_or_else(|| panic!("no data"));
    assert!(close(spread.std_dev_ns, 400.0 * MILLI as f64, 1e-12));
    let measured_phi = measured(jittery.suspicion(at(now + 2_200 * MILLI)));
    assert!(close(measured_phi, 2.869_699_035_929_369, 1e-9));
}

#[test]
fn refused_heartbeats_and_reads_do_not_mutate() {
    let mut detector = detector::<4>(normal(MILLI, 1));
    assert!(detector.heartbeat(at(10 * SECOND)).is_ok());
    assert!(detector.heartbeat(at(11 * SECOND)).is_ok());
    let before = detector.clone();
    assert_eq!(
        detector.heartbeat(at(11 * SECOND)),
        Err(Refusal::SameInstant)
    );
    assert_eq!(
        detector.heartbeat(at(11 * SECOND - 1)),
        Err(Refusal::TimeRegression)
    );
    assert_eq!(detector.heartbeat(Duration::MAX), Err(Refusal::Overflow));
    assert_eq!(
        detector.suspicion(at(10 * SECOND)),
        Err(Refusal::TimeRegression)
    );
    assert_eq!(detector.suspicion(Duration::MAX), Err(Refusal::Overflow));
    assert!(detector.suspicion(at(12 * SECOND)).is_ok());
    assert_eq!(detector, before);
}

#[test]
fn configuration_refusals_are_explicit() {
    let floor = |duration| Distribution::Normal {
        min_std_dev: duration,
    };
    assert_eq!(
        Config::new(Distribution::Exponential, 0),
        Err(ConfigError::ZeroWarmUp)
    );
    assert_eq!(
        Config::new(floor(Duration::ZERO), 1),
        Err(ConfigError::ZeroDeviationFloor)
    );
    assert_eq!(
        Config::new(floor(Duration::MAX), 1),
        Err(ConfigError::DurationOverflow)
    );
    assert_eq!(
        exponential(1).with_acceptable_pause(Duration::MAX),
        Err(ConfigError::DurationOverflow)
    );
    assert_eq!(
        Detector::<0>::new(exponential(1)),
        Err(ConfigError::ZeroWindow)
    );
    assert_eq!(
        Detector::<4>::new(exponential(5)),
        Err(ConfigError::WarmUpExceedsWindow)
    );
    assert!(Detector::<4>::new(exponential(4)).is_ok());
    assert_eq!(
        Roster::<2, 0>::new(exponential(1)),
        Err(ConfigError::ZeroWindow)
    );
    assert_eq!(
        Roster::<2, 4>::new(exponential(5)),
        Err(ConfigError::WarmUpExceedsWindow)
    );
    let config = normal(7, 3);
    assert_eq!(config.min_samples(), 3);
    assert_eq!(config.distribution(), floor(at(7)));
    assert_eq!(config.acceptable_pause(), Duration::ZERO);
    assert_eq!(Threshold::new(0.0), None);
    assert_eq!(Threshold::new(-1.0), None);
    assert_eq!(Threshold::new(f64::NAN), None);
    assert_eq!(Threshold::new(f64::INFINITY), None);
    assert_eq!(threshold(8.0).value(), 8.0);
}

#[test]
fn u64_limits_stay_finite_and_exact() {
    let mut detector = detector::<2>(normal(1, 1));
    assert!(detector.heartbeat(at(0)).is_ok());
    assert_eq!(
        detector.heartbeat(at(u64::MAX - 1)),
        Ok(Beat::Recorded {
            interval: at(u64::MAX - 1),
            samples: 1,
        })
    );
    assert_eq!(
        detector.heartbeat(at(u64::MAX)),
        Ok(Beat::Recorded {
            interval: at(1),
            samples: 2,
        })
    );
    assert_eq!(detector.heartbeat(at(u64::MAX)), Err(Refusal::SameInstant));
    let statistics = detector.statistics().unwrap_or_else(|| panic!("no data"));
    assert_eq!(statistics.max, at(u64::MAX - 1));
    assert_eq!(statistics.min, at(1));
    assert!(statistics.mean_ns.is_finite() && statistics.std_dev_ns.is_finite());
    assert!(measured(detector.suspicion(at(u64::MAX))).is_finite());
    assert_eq!(
        detector.suspicion(Duration::from_nanos(u64::MAX) + at(1)),
        Err(Refusal::Overflow)
    );

    // The steepest reachable slope: 1 ns intervals, 1 ns floor, 584 years late.
    let mut steep = Detector::<2>::new(normal(1, 1)).unwrap_or_else(|error| panic!("{error:?}"));
    assert!(steep.heartbeat(at(0)).is_ok());
    assert!(steep.heartbeat(at(1)).is_ok());
    let Ok(Suspicion::Measured(phi)) = steep.suspicion(at(u64::MAX)) else {
        panic!("expected phi");
    };
    assert!(phi.value().is_finite() && phi.value() > 1e37);
    assert_eq!(phi.millis(), u64::MAX);
    assert!(phi.reaches(threshold(1e30)));
    assert_eq!(Phi::default().millis(), 0);
    let Ok(Suspicion::Measured(mild)) = steep.suspicion(at(2)) else {
        panic!("expected phi");
    };
    assert_eq!(mild.millis(), 301);
    assert!(!mild.reaches(threshold(1.0)));
}

#[test]
fn detector_matches_naive_model_on_random_schedules() {
    const WINDOW: usize = 16;
    let mut rng = Rng::new(0xD17E_C704);
    let mut compared = 0;
    for _ in 0..40 {
        let config = random_config(&mut rng, WINDOW);
        let mut detector = detector::<WINDOW>(config);
        let mut model = Model::default();
        let mut now = rng.below(SECOND);
        for _ in 0..120 {
            let stamp = if rng.below(12) == 0 {
                now.saturating_sub(rng.below(2 * SECOND))
            } else {
                now += random_gap(&mut rng);
                now
            };
            let before = detector.clone();
            let outcome = detector.heartbeat(at(stamp));
            assert_eq!(outcome, model.observe(stamp, WINDOW));
            if outcome.is_err() {
                assert_eq!(detector, before);
            }
            assert_eq!(detector.samples(), model.intervals.len());
            if let Some(statistics) = detector.statistics() {
                assert!(close(statistics.mean_ns, model.mean(), 1e-12));
                assert!(close(statistics.std_dev_ns, model.std_dev(), 1e-9));
            } else {
                assert_eq!(model.intervals.len(), 0);
            }
            let probe = (now + rng.below(4 * SECOND)).saturating_sub(rng.below(SECOND));
            match (detector.suspicion(at(probe)), model.phi(&config, probe)) {
                (Err(Refusal::TimeRegression), Expected::Refused)
                | (Ok(Suspicion::NoHeartbeat | Suspicion::WarmingUp { .. }), Expected::Undefined) =>
                    {}
                (Ok(Suspicion::Measured(phi)), Expected::Phi(expected)) => {
                    assert!(
                        close(phi.value(), expected, 1e-8),
                        "{} vs {expected} under {config:?}",
                        phi.value()
                    );
                    compared += 1;
                }
                (actual, expected) => panic!("{actual:?} vs {expected:?}"),
            }
        }
    }
    assert!(compared > 1_000, "only {compared} phi comparisons");
}
