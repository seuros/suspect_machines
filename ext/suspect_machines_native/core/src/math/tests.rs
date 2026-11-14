#![allow(clippy::float_cmp, clippy::cast_precision_loss)]

use super::*;
use crate::test_support::{Rng, reference_normal_phi, within as close};

/// `-log10(Q(z))` computed with 80-digit decimal arithmetic.
const NORMAL_PHI: [(f64, f64); 20] = [
    (-8.0, 2.701_728_849_543_921_4e-16),
    (-3.0, 5.866_493_137_900_667e-4),
    (-1.0, 7.502_601_295_781_802e-2),
    (-0.25, 2.227_861_532_545_572_5e-1),
    (0.0, core::f64::consts::LOG10_2),
    (0.5, 5.106_919_892_652_407e-1),
    (1.0, 7.995_455_414_919_705e-1),
    (2.0, 1.643_016_080_140_937),
    (3.0, 2.869_699_035_929_369),
    (3.5, 3.633_335_998_656_066_5),
    (4.0, 4.499_334_907_556_479),
    (5.0, 6.542_645_672_390_654_5),
    (6.0, 9.005_864_327_476_704),
    (8.0, 1.520_614_255_101_715_5e1),
    (10.0, 2.311_805_340_548_607_7e1),
    (20.0, 8.856_009_534_307_559e1),
    (38.0, 3.155_397_897_039_625e2),
    (100.0, 2.173_871_542_869_034_3e3),
    (10_000.0, 2.171_472_849_425_253e7),
    (1_000_000_000.0, 2.171_472_409_516_259_2e17),
];

#[test]
fn sqrt_matches_std_within_documented_bound() {
    let mut rng = Rng::new(0x5EED_0001);
    for _ in 0..20_000 {
        let exponent = i32::try_from(rng.below(600)).expect("below 600") - 300;
        let x = (1.0 + rng.unit()) * 10f64.powi(exponent);
        assert!(close(sqrt(x), x.sqrt(), 0.0, 4e-16), "sqrt({x})");
    }
    for x in [1.0, 4.0, 2.0, 1e-320, f64::MIN_POSITIVE, f64::MAX, 3.4e38] {
        assert!(close(sqrt(x), x.sqrt(), 0.0, 4e-16), "sqrt({x})");
    }
    assert_eq!(sqrt(0.0), 0.0);
    assert_eq!(sqrt(-1.0), 0.0);
    assert_eq!(sqrt(f64::NAN), 0.0);
    assert_eq!(sqrt(f64::INFINITY), 0.0);
}

#[test]
fn exp_matches_std_within_documented_bound() {
    let mut rng = Rng::new(0x5EED_0002);
    for _ in 0..20_000 {
        let x = -708.0 + rng.unit() * 1417.0;
        assert!(close(exp(x), x.exp(), 0.0, 1e-14), "exp({x})");
    }
    for x in [0.0, -708.0, 709.0, 1e-300, -1e-300, LN_2, -LN_2 / 2.0] {
        assert!(close(exp(x), x.exp(), 0.0, 1e-14), "exp({x})");
    }
    assert_eq!(exp(0.0), 1.0);
    assert_eq!(exp(-709.0), 0.0);
    assert_eq!(exp(f64::NEG_INFINITY), 0.0);
    assert_eq!(exp(710.0), f64::INFINITY);
    assert!(exp(f64::NAN).is_nan());
}

#[test]
fn ln_matches_std_within_documented_bound() {
    let mut rng = Rng::new(0x5EED_0003);
    for _ in 0..20_000 {
        let exponent = i32::try_from(rng.below(600)).expect("below 600") - 300;
        let x = (1.0 + 9.0 * rng.unit()) * 10f64.powi(exponent);
        assert!(close(ln(x), x.ln(), 1e-15, 1e-14), "ln({x})");
        let near_one = 1.0 + (rng.unit() - 0.5) * 1e-3;
        assert!(close(ln(near_one), near_one.ln(), 1e-15, 1e-14));
    }
    for x in [f64::MIN_POSITIVE, f64::MAX, 1e-320, 2.0, 0.5] {
        assert!(close(ln(x), x.ln(), 1e-15, 1e-14), "ln({x})");
    }
    assert_eq!(ln(1.0), 0.0);
    assert_eq!(ln(0.0), f64::NEG_INFINITY);
    assert_eq!(ln(-1.0), f64::NEG_INFINITY);
    assert_eq!(ln(f64::INFINITY), f64::INFINITY);
    assert!(ln(f64::NAN).is_nan());
}

#[test]
fn normal_phi_matches_high_precision_table() {
    for (z, expected) in NORMAL_PHI {
        let actual = normal_phi(z);
        assert!(
            close(actual, expected, 1e-12, 1e-12),
            "phi({z}) = {actual}, expected {expected}"
        );
    }
}

#[test]
fn normal_phi_matches_independent_quadrature() {
    let mut rng = Rng::new(0x5EED_0004);
    for step in 0..400 {
        let z = -10.0 + f64::from(step) * 0.125;
        assert!(close(normal_phi(z), reference_normal_phi(z), 1e-9, 1e-9));
    }
    for _ in 0..300 {
        let z = 10f64.powf(rng.unit() * 12.0);
        assert!(close(normal_phi(z), reference_normal_phi(z), 1e-9, 1e-9));
    }
}

#[test]
fn normal_phi_is_finite_non_negative_and_non_decreasing() {
    let mut previous = 0.0_f64;
    for step in 0..20_000 {
        let z = -12.0 + f64::from(step) * 0.0025;
        let phi = normal_phi(z);
        assert!(phi.is_finite() && phi >= 0.0);
        // The two evaluation branches meet within the documented error.
        assert!(phi >= previous - 1e-12 * previous.max(1.0), "at z = {z}");
        previous = phi;
    }
    assert_eq!(normal_phi(-40.0), 0.0);
    let extreme = normal_phi(u64::MAX as f64);
    assert!(extreme.is_finite() && extreme > 1e37);
}

#[test]
fn exponential_phi_is_ratio_over_ln_ten() {
    assert_eq!(exponential_phi(0.0), 0.0);
    assert_eq!(exponential_phi(-3.0), 0.0);
    assert_eq!(exponential_phi(LN_10), 1.0);
    assert!(close(exponential_phi(23.0), 23.0 / LN_10, 0.0, 1e-15));
}
