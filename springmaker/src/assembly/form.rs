//! Assembly form state, dynamic member list, parsing, and solve routing.
//! iced-free per ADR 0008.

use springcore::assembly::{solve_assembly, AssemblyDesign, AssemblyInputs, AssemblyMember};
use springcore::units::{Force, Length};
use springcore::{
    parse_end_type, parse_fixity, parse_topology, AssemblyMemberSpec, AssemblySpec,
    CurvatureCorrection, MaterialStore, Result, UnitSystem,
};

use crate::form_helpers::{length_mm, loads_n, positive_num};

/// One member's editable text field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberField {
    WireDia,
    MeanDia,
    Active,
    FreeLength,
}

/// One member's form inputs (all strings; material/end-type via pickers).
#[derive(Debug, Clone)]
pub struct AsmMemberForm {
    pub material: String,
    pub end_type: String,
    pub wire_dia: String,
    pub mean_dia: String,
    pub active: String,
    pub free_length: String,
}

impl AsmMemberForm {
    /// A blank member card seeded with the given default material.
    pub fn blank(default_material: &str) -> Self {
        Self {
            material: default_material.to_string(),
            end_type: "squared_ground".into(),
            wire_dia: String::new(),
            mean_dia: String::new(),
            active: String::new(),
            free_length: String::new(),
        }
    }

    fn is_blank(&self) -> bool {
        [
            &self.wire_dia,
            &self.mean_dia,
            &self.active,
            &self.free_length,
        ]
        .iter()
        .all(|f| f.trim().is_empty())
    }
}

/// Assembly form state.
#[derive(Debug, Clone)]
pub struct AsmFormState {
    pub topology: String,
    pub fixity: String,
    pub loads: String,
    pub members: Vec<AsmMemberForm>,
}

impl AsmFormState {
    /// A fresh form opens with one blank member (the min-one floor).
    pub fn with_default_material(default_material: &str) -> Self {
        Self {
            topology: "nested".into(),
            fixity: "fixed_fixed".into(),
            loads: String::new(),
            members: vec![AsmMemberForm::blank(default_material)],
        }
    }

    pub fn is_blank(&self) -> bool {
        self.loads.trim().is_empty() && self.members.iter().all(AsmMemberForm::is_blank)
    }
}

/// Parse the whole form and solve. Wires `parse_topology`/`parse_fixity`
/// (the topology-rejection pin lands here) and threads the app-global
/// curvature correction (the compression pattern).
pub fn parse_and_solve(
    form: &AsmFormState,
    us: UnitSystem,
    materials: &MaterialStore,
    correction: CurvatureCorrection,
) -> Result<AssemblyDesign> {
    let topology = parse_topology(&form.topology)?;
    let fixity = parse_fixity(&form.fixity)?;
    let mut members = Vec::with_capacity(form.members.len());
    for m in &form.members {
        members.push(AssemblyMember {
            material_name: m.material.clone(),
            wire_dia: Length::from_millimeters(length_mm("wire diameter", &m.wire_dia, us)?),
            mean_dia: Length::from_millimeters(length_mm("mean diameter", &m.mean_dia, us)?),
            active_coils: positive_num("active coils", &m.active)?,
            free_length: Length::from_millimeters(length_mm("free length", &m.free_length, us)?),
            end_type: parse_end_type(&m.end_type)?,
        });
    }
    let loads: Vec<Force> = loads_n(&form.loads, us)?
        .into_iter()
        .map(Force::from_newtons)
        .collect();
    solve_assembly(
        materials,
        &AssemblyInputs { topology, members },
        &loads,
        fixity,
        correction,
    )
}

/// Build the persisted spec from the form.
pub fn build_spec(form: &AsmFormState, us: UnitSystem) -> Result<AssemblySpec> {
    let members = form
        .members
        .iter()
        .map(|m| {
            Ok(AssemblyMemberSpec {
                material_name: m.material.clone(),
                end_type: m.end_type.clone(),
                wire_dia_mm: length_mm("wire diameter", &m.wire_dia, us)?,
                mean_dia_mm: length_mm("mean diameter", &m.mean_dia, us)?,
                active: positive_num("active coils", &m.active)?,
                free_length_mm: length_mm("free length", &m.free_length, us)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(AssemblySpec::PowerUser {
        topology: form.topology.clone(),
        fixity: form.fixity.clone(),
        loads_n: loads_n(&form.loads, us)?,
        members,
    })
}

/// Fill the form from a loaded spec (round-trips with `build_spec`).
pub fn populate_from_spec(form: &mut AsmFormState, spec: &AssemblySpec, us: UnitSystem) {
    let AssemblySpec::PowerUser {
        topology,
        fixity,
        loads_n,
        members,
    } = spec;
    form.topology = topology.clone();
    form.fixity = fixity.clone();
    form.loads = crate::form_helpers::fmt_loads(loads_n, us);
    form.members = members
        .iter()
        .map(|m| AsmMemberForm {
            material: m.material_name.clone(),
            end_type: m.end_type.clone(),
            wire_dia: crate::form_helpers::fmt_len(m.wire_dia_mm, us),
            mean_dia: crate::form_helpers::fmt_len(m.mean_dia_mm, us),
            active: format!("{}", m.active),
            free_length: crate::form_helpers::fmt_len(m.free_length_mm, us),
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> MaterialStore {
        MaterialStore::new(springcore::MaterialSet::load_default())
    }

    fn two_member_form() -> AsmFormState {
        let mut f = AsmFormState::with_default_material("Music Wire");
        f.loads = "10, 25".into();
        f.members[0] = AsmMemberForm {
            wire_dia: "2".into(),
            mean_dia: "20".into(),
            active: "10".into(),
            free_length: "60".into(),
            ..AsmMemberForm::blank("Music Wire")
        };
        f.members.push(AsmMemberForm {
            wire_dia: "1.5".into(),
            mean_dia: "16".into(),
            active: "8".into(),
            free_length: "60".into(),
            ..AsmMemberForm::blank("Music Wire")
        });
        f
    }

    #[test]
    fn golden_through_form_matches_direct_solve() {
        let out = parse_and_solve(
            &two_member_form(),
            UnitSystem::Metric,
            &store(),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_eq!(out.members.len(), 2);
        assert_eq!(out.topology, springcore::assembly::Topology::Nested);
        // combined nested rate = k1 + k2 (both members solved)
        let k: f64 = out
            .members
            .iter()
            .map(|m| m.design.rate.newtons_per_meter())
            .sum();
        approx::assert_relative_eq!(out.rate.newtons_per_meter(), k, max_relative = 1e-12);
    }

    #[test]
    fn topology_rejection_is_end_to_end() {
        // THE engine-panel carry-forward: a bad topology (from a loaded file)
        // now rejects through parse_and_solve, not just parse_topology.
        let mut f = two_member_form();
        f.topology = "stacked".into();
        let err = parse_and_solve(
            &f,
            UnitSystem::Metric,
            &store(),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("unknown topology: stacked"),
            "got: {err}"
        );
    }

    #[test]
    fn build_populate_round_trips() {
        for us in [UnitSystem::Metric, UnitSystem::Us] {
            let mut f = two_member_form();
            if us == UnitSystem::Us {
                for m in &mut f.members {
                    m.wire_dia = "0.08".into();
                    m.mean_dia = "0.8".into();
                    m.free_length = "2.4".into();
                }
                f.loads = "2, 5".into();
            }
            let spec = build_spec(&f, us).unwrap();
            let mut round = AsmFormState::with_default_material("Music Wire");
            populate_from_spec(&mut round, &spec, us);
            assert_eq!(
                build_spec(&round, us).unwrap(),
                spec,
                "round-trip lossless ({us:?})"
            );
        }
    }

    #[test]
    fn is_blank_default_and_member_material_governs() {
        assert!(AsmFormState::with_default_material("Music Wire").is_blank());
        // Decision-2: a member's own material name is what solves.
        let mut f = two_member_form();
        f.members[1].material = "Stainless 302".into();
        let out = parse_and_solve(
            &f,
            UnitSystem::Metric,
            &store(),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        assert_eq!(out.members[1].material_name, "Stainless 302");
    }

    #[test]
    fn member_diameter_error_is_member_scoped() {
        let mut f = two_member_form();
        // Use Series so the shared-free-length guard doesn't fire before the
        // per-member diameter guard (nested requires equal free lengths).
        f.topology = "series".into();
        f.members[1].wire_dia = "10".into(); // out of range for music wire
        f.members[1].mean_dia = "80".into();
        f.members[1].free_length = "200".into();
        let err = parse_and_solve(
            &f,
            UnitSystem::Metric,
            &store(),
            CurvatureCorrection::Bergstrasser,
        )
        .unwrap_err();
        assert!(
            matches!(err, springcore::SpringError::Member { index: 1, .. }),
            "got: {err:?}"
        );
    }
}
