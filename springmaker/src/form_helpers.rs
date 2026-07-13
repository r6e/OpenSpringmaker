//! Form-parsing helpers shared across spring families.
//!
//! Pure functions that parse raw strings into SI values and format SI values
//! back to display strings. No iced dependency; each helper is unit-testable.

use springcore::units::{Force, Length, Moment, SpringRate};
use springcore::UnitSystem;
use springcore::{Result, SpringError};

/// Conversion factor: N/mm displayed ↔ N/m stored internally.
pub(crate) const MM_PER_M: f64 = 1000.0;

/// Render a [`SpringError`] with length values expressed in `units`.
///
/// `DiameterOutOfRange` and `FreeLengthBelowMinimum` bake SI metres into
/// their `Display` impls. This function converts those lengths to the
/// active unit system before formatting so US-customary users see inches
/// rather than metres.
///
/// All other variants have unit-neutral messages, so we fall through to
/// `err.to_string()`.
pub fn format_error(err: &SpringError, units: UnitSystem) -> String {
    match err {
        SpringError::DiameterOutOfRange {
            diameter_m,
            min_m,
            max_m,
        } => match units {
            UnitSystem::Metric => {
                let d = Length::from_meters(*diameter_m).millimeters();
                let lo = Length::from_meters(*min_m).millimeters();
                let hi = Length::from_meters(*max_m).millimeters();
                format!("wire diameter {d:.3} mm is outside the valid range [{lo:.3}, {hi:.3}] mm")
            }
            UnitSystem::Us => {
                let d = Length::from_meters(*diameter_m).inches();
                let lo = Length::from_meters(*min_m).inches();
                let hi = Length::from_meters(*max_m).inches();
                format!("wire diameter {d:.3} in is outside the valid range [{lo:.3}, {hi:.3}] in")
            }
        },
        SpringError::FreeLengthBelowMinimum {
            free_length_m,
            min_free_length_m,
        } => match units {
            UnitSystem::Metric => below_minimum_message(
                Length::from_meters(*free_length_m).millimeters(),
                Length::from_meters(*min_free_length_m).millimeters(),
                "mm",
            ),
            UnitSystem::Us => below_minimum_message(
                Length::from_meters(*free_length_m).inches(),
                Length::from_meters(*min_free_length_m).inches(),
                "in",
            ),
        },
        SpringError::Member { index, source } => {
            let inner = match source.as_ref() {
                SpringError::DiameterOutOfRange { .. }
                | SpringError::FreeLengthBelowMinimum { .. } => format_error(source, units),
                SpringError::InconsistentInputs(m) => m.clone(),
                other => other.to_string(),
            };
            format!("member {}: {inner}", index + 1)
        }
        // All other variants carry unit-neutral messages.
        other => other.to_string(),
    }
}

/// The free-length-below-minimum message in one display unit. When the two
/// values round to the SAME 3-decimal rendering (a free length within half
/// a display quantum below the minimum), the plain message would read
/// "X is below X" — self-contradictory (R2 stateful-UI F2) — so the deficit
/// is appended in scientific notation, which never rounds to zero for a
/// genuinely rejected (strictly smaller) value.
///
/// The engine guards the SI-metre values finite, but the metre→display scale
/// (`×1000` for mm) can push a finite-but-astronomical minimum past
/// `f64::MAX` to ±∞ (R3 input-domain F-R3-1: e.g. `active_coils = 3e307`
/// yields `min ≈ 1.92e305 m`, and `×1000` overflows). Rendering "…minimum
/// inf mm" is garbage, so a non-finite converted value falls back to an
/// out-of-range phrasing that carries no bogus number. (Display-layer,
/// defense-in-depth fix — the finding's minimum-required remedy.)
fn below_minimum_message(free: f64, min: f64, unit: &str) -> String {
    if !free.is_finite() || !min.is_finite() {
        return format!(
            "free length is below the close-wound minimum, but the value is \
             out of the displayable range in {unit} (input out of range)"
        );
    }
    let free_s = format!("{free:.3}");
    let min_s = format!("{min:.3}");
    let base =
        format!("free length {free_s} {unit} is below the close-wound minimum {min_s} {unit}");
    if free_s == min_s {
        format!("{base} (short by {:.3e} {unit})", min - free)
    } else {
        base
    }
}

/// Parse a single numeric field; return `InconsistentInputs` on failure.
///
/// Rejects empty strings, non-numeric text, and non-finite values (±∞, NaN).
pub(crate) fn num(field: &str, value: &str) -> Result<f64> {
    let v = value.trim().parse::<f64>().map_err(|_| {
        SpringError::InconsistentInputs(format!("{field} is not a number: '{value}'"))
    })?;
    if !v.is_finite() {
        return Err(SpringError::InconsistentInputs(format!(
            "{field} must be a finite number: '{value}'"
        )));
    }
    Ok(v)
}

/// Like `num`, but additionally requires the value to be strictly greater than zero.
pub(crate) fn positive_num(field: &str, value: &str) -> Result<f64> {
    let v = num(field, value)?;
    if v <= 0.0 {
        return Err(SpringError::InconsistentInputs(format!(
            "{field} must be greater than zero"
        )));
    }
    Ok(v)
}

/// Return `v_si` if finite, else a field-named error. Centralizes the finiteness
/// guard shared by the unit-converting helpers AND derived-computation call sites
/// (e.g. the torsion force-at-radius moment, F·r): a finite display value can
/// overflow to ±Inf after the US/metric scale factor or a derived computation, so
/// each caller re-checks the value it computed here.
pub(crate) fn finite_or_err(field: &str, value: &str, v_si: f64) -> Result<f64> {
    if v_si.is_finite() {
        Ok(v_si)
    } else {
        // The display value already passed num's finiteness check, so the only way
        // to reach here is the computed value (a unit conversion or a derivation
        // like F·r) overflowing to ±Inf — report that, not a misleading "not a
        // finite number" (the user's input was finite).
        Err(SpringError::InconsistentInputs(format!(
            "{field} is too large: '{value}' overflows the computed value"
        )))
    }
}

/// Shared core: parse a strictly-positive value, convert to SI via
/// `convert_us` (US) or pass through unchanged (metric), then
/// finiteness-check the converted result.
fn positive_to_si(
    field: &str,
    value: &str,
    us: UnitSystem,
    convert_us: impl Fn(f64) -> f64,
) -> Result<f64> {
    let v = positive_num(field, value)?;
    let v_si = match us {
        UnitSystem::Us => convert_us(v),
        UnitSystem::Metric => v,
    };
    finite_or_err(field, value, v_si)
}

/// Shared core: like `positive_to_si` but allows zero, rejecting negatives
/// with the "zero or greater" message.
fn non_negative_to_si(
    field: &str,
    value: &str,
    us: UnitSystem,
    convert_us: impl Fn(f64) -> f64,
) -> Result<f64> {
    let v = num(field, value)?;
    if v < 0.0 {
        return Err(SpringError::InconsistentInputs(format!(
            "{field} must be zero or greater"
        )));
    }
    let v_si = match us {
        UnitSystem::Us => convert_us(v),
        UnitSystem::Metric => v,
    };
    finite_or_err(field, value, v_si)
}

/// Parse a strictly-positive length, returning millimetres (SI internal):
/// US inputs are converted from inches, metric inputs are already mm.
pub(crate) fn length_mm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    // Lengths must be strictly positive — a zero-length dimension is unphysical.
    positive_to_si(field, value, us, |v| Length::from_inches(v).millimeters())
}

/// Like `length_mm` but allows zero (e.g. torsion spring legs that may be absent).
pub(crate) fn non_negative_length_mm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    non_negative_to_si(field, value, us, |v| Length::from_inches(v).millimeters())
}

/// Like `num` but requires the value to be >= 0 (zero allowed, negative rejected).
pub(crate) fn non_negative_force_n(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    non_negative_to_si(field, value, us, |v| Force::from_pounds_force(v).newtons())
}

/// Like `num` but requires the value to be >= 0 (zero allowed, negative rejected),
/// returning SI newton-millimetres. Zero is legal for cycle-moment minimums — the
/// exact R = 0 repeated-bending case the fatigue data is defined for.
pub(crate) fn non_negative_moment_nmm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    non_negative_to_si(field, value, us, |v| {
        Moment::from_pound_force_inches(v).newton_millimeters()
    })
}

/// Like `non_negative_force_n` but requires the value to be strictly positive
/// (e.g. max force, which must be greater than zero).
pub(crate) fn positive_force_n(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    positive_to_si(field, value, us, |v| Force::from_pounds_force(v).newtons())
}

// NOTE: deliberately not on `positive_to_si` — the metric arm scales
// (N/mm display → N/m stored); it is not an identity pass-through.
pub(crate) fn rate_npm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    // A spring rate must be strictly positive.
    // Metric input is in N/mm (display unit); convert to N/m for internal storage.
    let v = positive_num(field, value)?;
    let v_si = match us {
        UnitSystem::Us => SpringRate::from_pounds_per_inch(v).newtons_per_meter(),
        UnitSystem::Metric => v * MM_PER_M,
    };
    finite_or_err(field, value, v_si)
}

/// Parse a strictly-positive angular rate, returning N·mm per degree (canonical):
/// metric input is already N·mm/°; US input is lbf·in/°, converted via `Moment`.
pub(crate) fn ang_rate_nmm_per_deg(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    positive_to_si(field, value, us, |v| {
        Moment::from_pound_force_inches(v).newton_millimeters()
    })
}

/// Convert N·mm/° (canonical) → display string (metric N·mm/°, US lbf·in/°).
pub(crate) fn fmt_ang_rate_nmm_per_deg(nmm_per_deg: f64, us: UnitSystem) -> String {
    match us {
        UnitSystem::Metric => format!("{nmm_per_deg}"),
        UnitSystem::Us => format!(
            "{}",
            Moment::from_newton_millimeters(nmm_per_deg).pound_force_inches()
        ),
    }
}

pub(crate) fn loads_n(value: &str, us: UnitSystem) -> Result<Vec<f64>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| non_negative_force_n("load", s, us))
        .collect()
}

/// Convert mm (SI internal) → display string.
pub(crate) fn fmt_len(mm: f64, us: UnitSystem) -> String {
    match us {
        UnitSystem::Metric => format!("{mm}"),
        UnitSystem::Us => format!("{}", Length::from_millimeters(mm).inches()),
    }
}

/// Convert N → display string.
pub(crate) fn fmt_force(n: f64, us: UnitSystem) -> String {
    match us {
        UnitSystem::Metric => format!("{n}"),
        UnitSystem::Us => format!("{}", Force::from_newtons(n).pounds_force()),
    }
}

/// Convert N/m (internal storage) → display string.
/// Metric: N/m internal → N/mm display (÷ MM_PER_M); US: N/m → lbf/in.
pub(crate) fn fmt_rate(npm: f64, us: UnitSystem) -> String {
    match us {
        UnitSystem::Metric => format!("{}", npm / MM_PER_M),
        UnitSystem::Us => format!(
            "{}",
            SpringRate::from_newtons_per_meter(npm).pounds_per_inch()
        ),
    }
}

/// Join a slice of newtons values → comma-separated display string.
pub(crate) fn fmt_loads(loads: &[f64], us: UnitSystem) -> String {
    loads
        .iter()
        .map(|&n| fmt_force(n, us))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parse a strictly-positive moment, returning newton-millimetres (SI internal):
/// US inputs are lbf·in, metric inputs are already N·mm. Moments must be > 0
/// (a torsion load winds the coils tighter).
pub(crate) fn moment_nmm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    positive_to_si(field, value, us, |v| {
        Moment::from_pound_force_inches(v).newton_millimeters()
    })
}

/// Parse a comma-separated moment list into SI newton-millimetres.
pub(crate) fn moments_nmm(value: &str, us: UnitSystem) -> Result<Vec<f64>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| moment_nmm("moment", s, us))
        .collect()
}

/// Convert N·mm (SI internal) → display string.
pub(crate) fn fmt_moment(nmm: f64, us: UnitSystem) -> String {
    match us {
        UnitSystem::Metric => format!("{nmm}"),
        UnitSystem::Us => format!(
            "{}",
            Moment::from_newton_millimeters(nmm).pound_force_inches()
        ),
    }
}

/// Join a slice of N·mm values → comma-separated display string.
pub(crate) fn fmt_moments(moments: &[f64], us: UnitSystem) -> String {
    moments
        .iter()
        .map(|&m| fmt_moment(m, us))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parse an angle in degrees: any FINITE number (TwoLoad is offset-tolerant, so
/// negative and zero angles are legal). Degrees in both unit systems — no conversion.
pub(crate) fn angle_deg(field: &str, value: &str) -> Result<f64> {
    num(field, value)
}

/// Format an angle in degrees for form population.
pub(crate) fn fmt_angle_deg(deg: f64) -> String {
    format!("{deg}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use springcore::SpringError;

    #[test]
    fn moment_nmm_metric_passthrough_and_positive() {
        assert_eq!(
            moment_nmm("moment", "100", UnitSystem::Metric).unwrap(),
            100.0
        );
        assert!(moment_nmm("moment", "0", UnitSystem::Metric).is_err()); // must be > 0
        assert!(moment_nmm("moment", "-1", UnitSystem::Metric).is_err());
    }

    #[test]
    fn moment_nmm_us_converts_lbf_in_to_nmm() {
        // 1 lbf·in = 4.4482216152605 N × 0.0254 m = 0.112984829... N·m = 112.984829 N·mm.
        let v = moment_nmm("moment", "1", UnitSystem::Us).unwrap();
        approx::assert_relative_eq!(v, 4.4482216152605 * 0.0254 * 1000.0, max_relative = 1e-9);
    }

    #[test]
    fn moments_nmm_parses_comma_list_and_fmt_moments_round_trips_metric() {
        let v = moments_nmm("100, 250", UnitSystem::Metric).unwrap();
        assert_eq!(v, vec![100.0, 250.0]);
        assert_eq!(fmt_moments(&v, UnitSystem::Metric), "100, 250");
    }

    #[test]
    fn fmt_moments_us_round_trips() {
        // Parse a US "1, 2" lbf·in list to SI, then format back: the display
        // string must reproduce the input (round-trip coverage for US units,
        // locally rather than only transitively through the form-level test).
        let v = moments_nmm("1, 2", UnitSystem::Us).unwrap();
        assert_eq!(fmt_moments(&v, UnitSystem::Us), "1, 2");
    }

    /// A US-unit length of "1e308" (inches) overflows to +Inf after ×25.4 conversion.
    /// The post-conversion `is_finite()` guard must catch this and return `Err`, not `Ok(+Inf)`.
    #[test]
    fn length_mm_us_overflow_to_inf_is_rejected() {
        let result = length_mm("test field", "1e308", UnitSystem::Us);
        let Err(SpringError::InconsistentInputs(msg)) = result else {
            panic!("US length overflow to +Inf must be rejected; got {result:?}");
        };
        // The input "1e308" is itself finite — the error must name the conversion
        // overflow, not claim the user typed a non-finite number.
        assert!(
            msg.contains("overflow") && msg.contains("test field"),
            "overflow error should name the field and the overflow; got: {msg}"
        );
    }

    /// Same guard applies to the metric `rate_npm` branch (×1000): a finite metric
    /// rate that overflows after unit conversion must also be rejected.
    #[test]
    fn rate_npm_metric_overflow_to_inf_is_rejected() {
        let result = rate_npm("spring rate", "1e306", UnitSystem::Metric);
        let Err(SpringError::InconsistentInputs(msg)) = result else {
            panic!("metric rate overflow to +Inf must be rejected; got {result:?}");
        };
        // The ×1000 N/mm→N/m conversion overflows a finite input; the message must name
        // the conversion overflow, not claim the rate the user typed was non-finite.
        assert!(
            msg.contains("overflow") && msg.contains("spring rate"),
            "overflow error should name the field and the overflow; got: {msg}"
        );
    }

    /// Normal finite US and metric inputs must still pass through cleanly.
    #[test]
    fn length_mm_normal_inputs_are_accepted() {
        assert!(length_mm("test", "1.0", UnitSystem::Us).is_ok());
        assert!(length_mm("test", "1.0", UnitSystem::Metric).is_ok());
    }

    #[test]
    fn rate_npm_normal_inputs_are_accepted() {
        assert!(rate_npm("rate", "2.0", UnitSystem::Us).is_ok());
        assert!(rate_npm("rate", "2.0", UnitSystem::Metric).is_ok());
    }

    #[test]
    fn ang_rate_nmm_per_deg_metric_passthrough_and_positive() {
        assert_eq!(
            ang_rate_nmm_per_deg("rate", "100", UnitSystem::Metric).unwrap(),
            100.0
        );
        assert!(ang_rate_nmm_per_deg("rate", "0", UnitSystem::Metric).is_err());
        assert!(ang_rate_nmm_per_deg("rate", "-1", UnitSystem::Metric).is_err());
    }

    #[test]
    fn ang_rate_nmm_per_deg_us_converts_lbf_in_per_deg_to_nmm_per_deg() {
        // 1 lbf·in/° = Moment::from_pound_force_inches(1).newton_millimeters()
        //             = 4.4482216152605 N × 0.0254 m × 1000 = 112.984...N·mm/°
        let v = ang_rate_nmm_per_deg("rate", "1", UnitSystem::Us).unwrap();
        approx::assert_relative_eq!(v, 4.4482216152605 * 0.0254 * 1000.0, max_relative = 1e-9);
    }

    #[test]
    fn fmt_ang_rate_nmm_per_deg_metric_round_trips() {
        let nmm = 50.0;
        let s = fmt_ang_rate_nmm_per_deg(nmm, UnitSystem::Metric);
        let back = ang_rate_nmm_per_deg("rate", &s, UnitSystem::Metric).unwrap();
        approx::assert_relative_eq!(back, nmm, max_relative = 1e-12);
    }

    #[test]
    fn fmt_ang_rate_nmm_per_deg_us_round_trips() {
        // Pick a canonical N·mm/° value, format as US, parse back — must recover original.
        let nmm = 4.4482216152605 * 0.0254 * 1000.0; // ≈ 1 lbf·in/°
        let s = fmt_ang_rate_nmm_per_deg(nmm, UnitSystem::Us);
        let back = ang_rate_nmm_per_deg("rate", &s, UnitSystem::Us).unwrap();
        approx::assert_relative_eq!(back, nmm, max_relative = 1e-9);
    }

    #[test]
    fn angle_deg_accepts_any_finite_including_negative() {
        assert_eq!(angle_deg("angle", "-10").unwrap(), -10.0);
        assert_eq!(angle_deg("angle", "0").unwrap(), 0.0);
        assert_eq!(angle_deg("angle", "90").unwrap(), 90.0);
        assert!(angle_deg("angle", "nan").is_err());
        assert!(angle_deg("angle", "inf").is_err());
        assert!(angle_deg("angle", "abc").is_err());
    }

    #[test]
    fn fmt_angle_deg_round_trips() {
        let v = -42.5_f64;
        assert_eq!(angle_deg("angle", &fmt_angle_deg(v)).unwrap(), v);
    }

    #[test]
    fn non_negative_moment_allows_zero_rejects_negative_converts_us() {
        assert_eq!(
            non_negative_moment_nmm("fatigue min", "0", UnitSystem::Metric).unwrap(),
            0.0
        );
        let err = non_negative_moment_nmm("fatigue min", "-1", UnitSystem::Metric).unwrap_err();
        assert!(err
            .to_string()
            .contains("fatigue min must be zero or greater"));
        // 1 lbf·in = 112.98482... N·mm (the moment conversion, not force).
        assert_relative_eq!(
            non_negative_moment_nmm("fatigue max", "1", UnitSystem::Us).unwrap(),
            4.4482216152605 * 0.0254 * 1000.0,
            max_relative = 1e-9
        );
        assert!(non_negative_moment_nmm("fatigue min", "nan", UnitSystem::Metric).is_err());
    }

    /// R2 stateful-UI F1: the structured below-minimum reject renders in
    /// the ACTIVE unit system — mm for metric, inches for US — instead of
    /// the engine's SI meters (or the old baked-in millimeters).
    #[test]
    fn format_error_relocalizes_free_length_below_minimum() {
        // The V5 fixture values: free 56.8 mm, minimum 57.2133726647001 mm.
        let e = SpringError::FreeLengthBelowMinimum {
            free_length_m: 0.0568,
            min_free_length_m: 0.0572133726647001,
        };
        assert_eq!(
            format_error(&e, UnitSystem::Metric),
            "free length 56.800 mm is below the close-wound minimum 57.213 mm"
        );
        // 56.8 mm = 2.23622... in; 57.21337... mm = 2.25249... in.
        assert_eq!(
            format_error(&e, UnitSystem::Us),
            "free length 2.236 in is below the close-wound minimum 2.252 in"
        );
    }

    /// R2 stateful-UI F2: a free length within half a display quantum below
    /// the minimum rounds to the SAME 3-decimal string — the message must
    /// then surface the deficit rather than read "X is below X".
    #[test]
    fn format_error_below_minimum_boundary_rounding_shows_the_deficit() {
        let e = SpringError::FreeLengthBelowMinimum {
            free_length_m: 0.0572130,
            min_free_length_m: 0.0572133726647001,
        };
        let msg = format_error(&e, UnitSystem::Metric);
        assert_eq!(
            msg,
            "free length 57.213 mm is below the close-wound minimum 57.213 mm \
             (short by 3.727e-4 mm)"
        );
    }

    /// R3 input-domain F-R3-1: a finite-but-astronomical SI minimum (the
    /// value the compression solver emits for `active_coils = "3e307"`,
    /// ≈1.92e305 m) overflows the metre→mm `×1000` scale to +∞
    /// (`f64::MAX/1000 ≈ 1.798e305`). The display layer must NOT render
    /// "…close-wound minimum inf mm"; it falls back to an out-of-range
    /// phrasing carrying no non-finite number.
    #[test]
    fn format_error_below_minimum_astronomical_minimum_does_not_render_inf() {
        let e = SpringError::FreeLengthBelowMinimum {
            free_length_m: 0.06,         // ×1000 = 60 mm, finite
            min_free_length_m: 1.92e305, // ×1000 = +∞ mm
        };
        let msg = format_error(&e, UnitSystem::Metric);
        assert!(
            !msg.contains("inf") && !msg.contains("NaN"),
            "must not render a non-finite number; got: {msg}"
        );
        assert!(
            msg.contains("out of") && msg.contains("range"),
            "must fall back to an out-of-range phrasing; got: {msg}"
        );
    }

    /// Member-wrapped below-minimum errors (an assembly member's free <
    /// solid reject) relocalize exactly like member-wrapped diameter errors.
    #[test]
    fn format_error_relocalizes_a_member_below_minimum_error() {
        let e = SpringError::Member {
            index: 1,
            source: Box::new(SpringError::FreeLengthBelowMinimum {
                free_length_m: 0.020,
                min_free_length_m: 0.024,
            }),
        };
        assert_eq!(
            format_error(&e, UnitSystem::Metric),
            "member 2: free length 20.000 mm is below the close-wound minimum 24.000 mm"
        );
        let us = format_error(&e, UnitSystem::Us);
        assert!(
            us.starts_with("member 2: free length 0.787 in"),
            "got: {us}"
        );
    }

    #[test]
    fn format_error_relocalizes_a_member_diameter_error() {
        let inner = SpringError::DiameterOutOfRange {
            diameter_m: 0.010,
            min_m: 0.0002,
            max_m: 0.0064,
        };
        let e = SpringError::Member {
            index: 0,
            source: Box::new(inner),
        };
        // US: inches, member-prefixed.
        let us = format_error(&e, UnitSystem::Us);
        assert!(
            us.starts_with("member 1: wire diameter") && us.contains(" in "),
            "got: {us}"
        );
        // Metric: mm.
        let m = format_error(&e, UnitSystem::Metric);
        assert!(
            m.starts_with("member 1: wire diameter") && m.contains(" mm "),
            "got: {m}"
        );
        // An InconsistentInputs member source: raw message, no doubled prefix.
        let e = SpringError::Member {
            index: 1,
            source: Box::new(SpringError::InconsistentInputs(
                "mean diameter must be greater".into(),
            )),
        };
        assert_eq!(
            format_error(&e, UnitSystem::Us),
            "member 2: mean diameter must be greater"
        );
    }
}
