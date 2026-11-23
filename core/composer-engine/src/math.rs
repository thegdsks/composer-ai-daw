//! Platform independent math. Only add, multiply and floor are used so native
//! and wasm builds produce identical bits (libm sin and powf may differ).

/// Sine of `turns` full cycles, accurate to about 1e-6.
pub fn sin_turns(turns: f32) -> f32 {
    let mut t = turns - turns.floor();
    if t > 0.5 {
        t -= 1.0;
    }
    if t > 0.25 {
        t = 0.5 - t;
    } else if t < -0.25 {
        t = -0.5 - t;
    }
    let a = t * core::f32::consts::TAU;
    let a2 = a * a;
    // Taylor series to a^11 on [-pi/2, pi/2], Horner form.
    let p = 1.0 - a2 / 110.0;
    let p = 1.0 - a2 / 72.0 * p;
    let p = 1.0 - a2 / 42.0 * p;
    let p = 1.0 - a2 / 20.0 * p;
    let p = 1.0 - a2 / 6.0 * p;
    a * p
}

const SEMITONES: [f64; 12] = [
    1.0,
    1.059_463_094_359_295_3,
    1.122_462_048_309_373,
    1.189_207_115_002_721,
    1.259_921_049_894_873_2,
    1.334_839_854_170_034_4,
    core::f64::consts::SQRT_2,
    1.498_307_076_876_681_5,
    1.587_401_051_968_199_4,
    1.681_792_830_507_429,
    1.781_797_436_280_678_5,
    1.887_748_625_363_387,
];

/// 2^(semitones/12) from a literal table and exact doublings.
pub fn semitone_ratio(semitones: i32) -> f64 {
    let s = semitones.clamp(-480, 480);
    let mut r = SEMITONES[s.rem_euclid(12) as usize];
    let octaves = s.div_euclid(12);
    for _ in 0..octaves.abs() {
        r = if octaves > 0 { r * 2.0 } else { r / 2.0 };
    }
    r
}

/// 2^(cents/1200). Whole semitones come from the table, the rest from a short Taylor series
/// of e^x (x below 0.06, error near 1e-12) so only add and multiply are used.
pub fn cents_ratio(cents: i32) -> f64 {
    let semis = cents.div_euclid(100);
    let rem = f64::from(cents.rem_euclid(100));
    let x = rem * (core::f64::consts::LN_2 / 1200.0);
    let mut e = 1.0;
    for k in (1..=7).rev() {
        e = 1.0 + e * x / f64::from(k);
    }
    semitone_ratio(semis) * e
}

/// Equal temperament frequency of a MIDI pitch, A4 (69) = 440 Hz.
pub fn pitch_hz(pitch: u8) -> f64 {
    440.0 * semitone_ratio(i32::from(pitch) - 69)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_matches_known_points() {
        assert!(sin_turns(0.0).abs() < 1e-6);
        assert!((sin_turns(0.25) - 1.0).abs() < 1e-5);
        assert!((sin_turns(0.75) + 1.0).abs() < 1e-5);
        let h = core::f32::consts::FRAC_1_SQRT_2;
        assert!((sin_turns(1.125) - h).abs() < 1e-5);
        assert!((sin_turns(-0.125) + h).abs() < 1e-5);
    }

    #[test]
    fn cents_ratio_points() {
        assert_eq!(cents_ratio(0), 1.0);
        assert_eq!(cents_ratio(1200), 2.0);
        assert_eq!(cents_ratio(-1200), 0.5);
        assert!((cents_ratio(50) - 2f64.powf(50.0 / 1200.0)).abs() < 1e-11);
        assert!((cents_ratio(-37) - 2f64.powf(-37.0 / 1200.0)).abs() < 1e-11);
    }

    #[test]
    fn pitch_table() {
        assert_eq!(pitch_hz(69), 440.0);
        assert_eq!(pitch_hz(81), 880.0);
        assert!((pitch_hz(60) - 261.6256).abs() < 1e-3);
    }
}
