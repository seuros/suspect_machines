//! Deterministic `no_std` elementary functions for the suspicion level.
//!
//! `f64::sqrt`, `exp` and `ln` live in `std`, so the detector carries its own.
//! Every routine uses only IEEE-754 add, subtract, multiply and divide in a
//! fixed evaluation order with fixed iteration counts: no fused multiply-add,
//! no platform `libm`, no data-dependent loop exits. The same inputs give the
//! same bits on every target.
//!
//! Error bounds, checked by the crate's tests against `std` and against
//! 60-digit reference values:
//!
//! | routine | domain | bound |
//! |:--------|:-------|:------|
//! | [`sqrt`] | finite `x >= 0` | relative `4e-16` |
//! | [`exp`] | `-708 <= x <= 709` | relative `1e-14` |
//! | [`ln`] | positive normal `x` | `1e-15` absolute or `1e-14` relative |
//! | [`normal_phi`] | finite `z` | `1e-12` absolute or `1e-12` relative |
#![allow(
    clippy::suboptimal_flops,
    clippy::manual_midpoint,
    reason = "plain IEEE operations only: mul_add needs libm on bare metal"
)]

use core::f64::consts::{FRAC_1_SQRT_2, FRAC_2_SQRT_PI, LN_2, LN_10};

/// `ln(2 * sqrt(pi))`.
const LN_2_SQRT_PI: f64 = 1.265_512_123_484_645_4;
/// Below this `x = |z| / sqrt(2)` the tail comes from the `erf` series; at or
/// above it, from the continued fraction.
const SERIES_LIMIT: f64 = 2.0;
/// Terms of the `erf` series; enough for `x < SERIES_LIMIT`.
const SERIES_TERMS: u32 = 60;
/// Levels of the `erfc` continued fraction; enough for `x >= SERIES_LIMIT`.
const FRACTION_LEVELS: u32 = 60;
/// Newton steps for `sqrt` from a start within 25 % of the root.
const NEWTON_STEPS: u32 = 6;
/// Degree of the `exp` Taylor polynomial on `|r| <= ln(2) / 2`.
const EXP_DEGREE: u32 = 14;
/// Terms of the `atanh` series behind `ln`.
const LN_TERMS: u32 = 13;
/// Upper bits of `ln(2)`, with enough trailing zeros that multiples up to
/// `2^10` are exact.
const LN_2_HIGH: f64 = 6.931_471_803_691_238e-1;
/// `ln(2) - LN_2_HIGH`.
const LN_2_LOW: f64 = 1.908_214_929_270_587_7e-10;
/// The 52 stored mantissa bits of an `f64`.
const MANTISSA_MASK: u64 = 0x000F_FFFF_FFFF_FFFF;
/// Width of the stored mantissa: the exponent field starts above it.
const MANTISSA_BITS: u32 = 52;
/// Bits of `1.0`: biased exponent zero, empty mantissa.
const ONE_BITS: u64 = 0x3FF0_0000_0000_0000;
/// IEEE-754 double exponent bias.
const EXPONENT_BIAS: i32 = 1023;
/// Power of two that lifts every subnormal into the normal range.
const LIFT: i32 = 54;
/// `2^LIFT`.
const TWO_54: f64 = 18_014_398_509_481_984.0;
/// Below this `e^x` underflows to zero (`e^-708` is the last normal result).
const EXP_FLOOR: f64 = -708.0;
/// Above this `e^x` overflows to infinity.
const EXP_CEILING: f64 = 709.0;

/// Biased exponent field of `bits` (bits 52 to 62), as a signed integer.
fn exponent_field(bits: u64) -> i32 {
    let [.., low, high] = bits.to_le_bytes();
    i32::from((u16::from(high & 0x7F) << 4) | u16::from(low >> 4))
}

/// `2^power` for `-1022 <= power <= 1023`, the only powers the callers form.
fn power_of_two(power: i32) -> f64 {
    debug_assert!(
        (1 - EXPONENT_BIAS..=EXPONENT_BIAS).contains(&power),
        "callers form only normal powers of two"
    );
    f64::from_bits(u64::from((power + EXPONENT_BIAS).cast_unsigned()) << MANTISSA_BITS)
}

/// `(exponent, mantissa)` with `x = mantissa * 2^exponent` and `mantissa` in
/// `[1, 2)`, for a finite positive `x`, subnormals included.
fn decompose(x: f64) -> (i32, f64) {
    let (scaled, lift) = if x < f64::MIN_POSITIVE {
        (x * TWO_54, LIFT)
    } else {
        (x, 0)
    };
    let bits = scaled.to_bits();
    let mantissa = f64::from_bits((bits & MANTISSA_MASK) | ONE_BITS);
    (exponent_field(bits) - EXPONENT_BIAS - lift, mantissa)
}

/// Square root of a finite non-negative `x`; zero for anything else.
pub fn sqrt(x: f64) -> f64 {
    if x.is_nan() || x <= 0.0 || x.is_infinite() {
        return 0.0;
    }
    let (mut exponent, mut mantissa) = decompose(x);
    if exponent % 2 != 0 {
        mantissa *= 2.0;
        exponent -= 1;
    }
    let mut root = 0.5 + 0.5 * mantissa;
    let mut step = 0;
    while step < NEWTON_STEPS {
        root = 0.5 * (root + mantissa / root);
        step += 1;
    }
    root * power_of_two(exponent / 2)
}

/// `e^x`, saturating to zero below [`EXP_FLOOR`] and to infinity above
/// [`EXP_CEILING`].
pub fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x < EXP_FLOOR {
        return 0.0;
    }
    if x > EXP_CEILING {
        return f64::INFINITY;
    }
    let turns = x / LN_2;
    let nearest = if turns < 0.0 {
        turns - 0.5
    } else {
        turns + 0.5
    };
    #[allow(
        clippy::cast_possible_truncation,
        reason = "|nearest| <= 1024 after the range checks"
    )]
    let power = nearest as i32;
    // Two-part ln(2): the high part times `power` is exact, so the reduced
    // argument keeps full precision across the whole domain.
    let rest = (x - f64::from(power) * LN_2_HIGH) - f64::from(power) * LN_2_LOW;
    let mut sum = 1.0;
    let mut degree = EXP_DEGREE;
    while degree > 0 {
        sum = 1.0 + sum * rest / f64::from(degree);
        degree -= 1;
    }
    // Split the scale so that 2^1024 is never formed for results near the top.
    let half = power / 2;
    sum * power_of_two(half) * power_of_two(power - half)
}

/// Natural logarithm of a positive finite `x`; negative infinity for zero or
/// negative input.
pub fn ln(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    if x <= 0.0 {
        return f64::NEG_INFINITY;
    }
    let (power, mut mantissa) = decompose(x);
    let mut exponent = f64::from(power);
    if mantissa > core::f64::consts::SQRT_2 {
        mantissa *= 0.5;
        exponent += 1.0;
    }
    let t = (mantissa - 1.0) / (mantissa + 1.0);
    let square = t * t;
    let mut series = 0.0;
    let mut term = LN_TERMS;
    while term > 0 {
        term -= 1;
        series = 1.0 / f64::from(2 * term + 1) + series * square;
    }
    exponent * LN_2 + 2.0 * series * t
}

/// `erf(x)` for `0 <= x < SERIES_LIMIT`, from the all-positive series
/// `2/sqrt(pi) * e^(-x^2) * sum 2^n x^(2n+1) / (2n+1)!!`.
fn erf_series(x: f64) -> f64 {
    let double_square = 2.0 * x * x;
    let mut term = x;
    let mut sum = x;
    let mut index = 0;
    while index < SERIES_TERMS {
        term = term * double_square / f64::from(2 * index + 3);
        sum += term;
        index += 1;
    }
    FRAC_2_SQRT_PI * exp(-x * x) * sum
}

/// Denominator `f` of `erfc(x) = e^(-x^2) / (sqrt(pi) * f)` for
/// `x >= SERIES_LIMIT`: `f = x + (1/2)/(x + (2/2)/(x + (3/2)/(x + ...)))`.
fn erfc_fraction(x: f64) -> f64 {
    let mut value = x;
    let mut level = FRACTION_LEVELS;
    while level > 0 {
        value = x + (f64::from(level) * 0.5) / value;
        level -= 1;
    }
    value
}

/// `-log10(Q(z))`, where `Q` is the upper tail of the standard normal
/// distribution. Evaluated in the log domain, so it stays finite and accurate
/// long after `Q(z)` itself would underflow. Never negative.
pub fn normal_phi(z: f64) -> f64 {
    let late = z >= 0.0;
    let x = if late { z } else { -z } * FRAC_1_SQRT_2;
    let ln_tail = if x < SERIES_LIMIT {
        let erf = erf_series(x);
        ln(if late {
            0.5 * (1.0 - erf)
        } else {
            0.5 * (1.0 + erf)
        })
    } else {
        let upper = -x * x - ln(erfc_fraction(x)) - LN_2_SQRT_PI;
        if late { upper } else { ln(1.0 - exp(upper)) }
    };
    let phi = -ln_tail / LN_10;
    if phi > 0.0 { phi } else { 0.0 }
}

/// `-log10(e^(-ratio))` for the exponential tail: `ratio / ln(10)`.
pub fn exponential_phi(ratio: f64) -> f64 {
    if ratio > 0.0 { ratio / LN_10 } else { 0.0 }
}

#[cfg(test)]
mod tests;
