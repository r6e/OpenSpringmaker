//! Form-parsing helpers shared across spring families.
//!
//! Pure functions that parse raw strings into SI values and format SI values
//! back to display strings. No iced dependency; each helper is unit-testable.

use springcore::units::{Force, Length, SpringRate};
use springcore::UnitSystem;
use springcore::{Result, SpringError};

/// Conversion factor: N/mm displayed ↔ N/m stored internally.
pub(crate) const MM_PER_M: f64 = 1000.0;

/// Render a [`SpringError`] with length values expressed in `units`.
///
/// `DiameterOutOfRange` bakes SI metres into its `Display` impl. This
/// function converts those lengths to the active unit system before
/// formatting so US-customary users see inches rather than metres.
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
        // All other variants carry unit-neutral messages.
        other => other.to_string(),
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

/// Return `v_si` if finite, else a field-named error. Centralizes the post-conversion
/// finiteness guard shared by the unit-converting helpers: a finite display value can
/// overflow to ±Inf after the US/metric scale factor, so each helper re-checks its
/// converted SI result here.
fn finite_or_err(field: &str, value: &str, v_si: f64) -> Result<f64> {
    if v_si.is_finite() {
        Ok(v_si)
    } else {
        // The display value already passed num's finiteness check, so the only way to
        // reach here is the unit conversion overflowing to ±Inf — report that, not a
        // misleading "not a finite number" (the user's input was finite).
        Err(SpringError::InconsistentInputs(format!(
            "{field} is too large: '{value}' overflows after unit conversion"
        )))
    }
}

/// Parse a strictly-positive length, returning millimetres (SI internal):
/// US inputs are converted from inches, metric inputs are already mm.
pub(crate) fn length_mm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    // Lengths must be strictly positive — a zero-length dimension is unphysical.
    let v = positive_num(field, value)?;
    let v_si = match us {
        UnitSystem::Us => Length::from_inches(v).millimeters(),
        UnitSystem::Metric => v,
    };
    finite_or_err(field, value, v_si)
}

/// Like `num` but requires the value to be >= 0 (zero allowed, negative rejected).
pub(crate) fn non_negative_force_n(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    let v = num(field, value)?;
    if v < 0.0 {
        return Err(SpringError::InconsistentInputs(format!(
            "{field} must be zero or greater"
        )));
    }
    let v_si = match us {
        UnitSystem::Us => Force::from_pounds_force(v).newtons(),
        UnitSystem::Metric => v,
    };
    finite_or_err(field, value, v_si)
}

/// Like `non_negative_force_n` but requires the value to be strictly positive
/// (e.g. max force, which must be greater than zero).
pub(crate) fn positive_force_n(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    let v = positive_num(field, value)?;
    let v_si = match us {
        UnitSystem::Us => Force::from_pounds_force(v).newtons(),
        UnitSystem::Metric => v,
    };
    finite_or_err(field, value, v_si)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use springcore::SpringError;

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
        assert!(
            matches!(result, Err(SpringError::InconsistentInputs(_))),
            "metric rate overflow to +Inf must be rejected; got {result:?}"
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
}
