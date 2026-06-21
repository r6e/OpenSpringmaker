# PR (b) Curated Materials — Verified Data Table

Source-of-truth for the three materials added in sub-project 2, PR (b). Every
value is traceable to a source read during transcription (no recalled numbers).
Per the data-correctness contract (design spec §7): two independent sources +
an independent golden oracle per material.

## Sources used

- **MH** = *Machinery's Handbook*, 32nd ed., "Springs" / "Stresses in Springs":
  - **Table 20** "Moduli of Elasticity in Torsion and Tension of Spring Materials" (E, G, psi).
  - **p. 390** "Minimum Tensile Strength of Spring Wire by Diameter" (kpsi and MPa vs diameter).
  - **pp. 299–303** material descriptions (composition, ASTM, max temperature).
  - **Fig. 1–10 (pp. 304–307)** allowable working / bending design-stress curves; **Table 1 (p. 308)** correction factors.
- **ASTM B159** minimum tensile by diameter band (independent cross-check for phosphor bronze).
- **optimumspring.com** technical pages — design stress (% of min tensile) and max operating temperature.
- Strength models are **least-squares fits to the MH p. 390 points** (the same method Shigley's A/m constants use); residuals quoted below. Fits computed and reported, not recalled.

## Convention

Internal storage SI. `mts_units = "si_mpa_mm"` → strength coefficients evaluated with
d in **mm**, result in **MPa**. PowerLaw: `Sut = A / d^m` (coeffs `[A, m]`).
Polynomial: `Sut = Σ cᵢ·dⁱ` ascending (coeffs `[c0, c1, c2]`). E/G converted psi→GPa
(1 psi = 6.894757e-6 GPa); density lb/in³→kg/m³ (×27679.905); °F→°C.

## Materials

### 1. Hard-Drawn MB — ASTM A227
- `mts_form = power_law`, `mts_coefficients = [1767.9, 0.184]` (fit to MH p390, max residual **2.9%**, n=25)
- `valid_dia`: 0.51–12.70 mm (MH p390 extent, 0.020–0.500 in)
- `youngs_modulus_gpa = 197.2`, `shear_modulus_gpa = 79.3` (MH Table 20, 0.064–0.125 in band; E/G vary <1% across bands)
- `density_kg_per_m3 = 7861` (0.284 lb/in³, optimumspring)
- `allowable_pct_torsion = 0.40` (optimumspring; cross-checked vs MH Fig 1 ≈ 40% avg service)
- `allowable_pct_bending = 0.40`, `allowable_pct_set = 0.40` — **conservative placeholder** (ADR 0006); MH design stresses are diameter-dependent curves (Fig 1, 7), not % scalars
- `max_service_temp_c = 121.1` (250 °F, MH p299: "0 to 250 °F")
- endurance: **none** (no cited Zimmerli data for hard-drawn; reports NoData)
- **Golden oracle:** model at d=2.03 mm → MH p390 (0.080 in) = 227 kpsi = **1565 MPa** (assert within fit tol). Cross-check: MH steel tensiles fall within ASTM A227 Class I/II ranges (1014–2234 MPa).

### 2. Chrome-Vanadium — ASTM A231
(MH's chromium-vanadium spring-wire designation; A232 is the valve-spring-quality variant.)
- `mts_form = power_law`, `mts_coefficients = [1858.1, 0.163]` (fit to MH p390 "Cr-V Alloy", max residual **3.9%**, n=23)
- `valid_dia`: 0.81–12.70 mm
- `youngs_modulus_gpa = 196.5`, `shear_modulus_gpa = 77.2` (MH Table 20)
- `density_kg_per_m3 = 7861` (0.284 lb/in³)
- `allowable_pct_torsion = 0.45` (optimumspring)
- `allowable_pct_bending = 0.45`, `allowable_pct_set = 0.45` — conservative placeholder (ADR 0006)
- `max_service_temp_c = 218.3` (425 °F, MH p300)
- endurance: **none**
- **Golden oracle:** model at d=2.67 mm → MH p390 (0.105 in) = 229 kpsi = **1579 MPa**. Cross-check: within ASTM A231/A232 tensile range (~1414–2000 MPa).

### 3. Phosphor Bronze — ASTM B159 (Grade A, CDA 510, 95Cu/5Sn)
- `mts_form = polynomial`, `mts_coefficients = [966.2, -37.85, 1.164]` (deg-2 fit to MH p390, max residual **3.3%**, n=8; power-law is a poor fit here, 9.6%, because the data caps flat at small d then declines)
- `valid_dia`: 0.10–11.10 mm
- `youngs_modulus_gpa = 103.4`, `shear_modulus_gpa = 41.4` (MH Table 20, "Phosphor Bronze 5 percent tin")
- `density_kg_per_m3 = 8858` (0.32 lb/in³, optimumspring)
- `allowable_pct_torsion = 0.40` (optimumspring; consistent with MH Table 1 bronze factor 0.45–0.55)
- `allowable_pct_bending = 0.40`, `allowable_pct_set = 0.40` — conservative placeholder (ADR 0006)
- `max_service_temp_c = 100.0` (212 °F, MH p301: "up to 212 °F")
- endurance: **none**
- **Golden oracle (strongest):** model at d=1.04 mm → MH p390 (0.041 in) = 135 kpsi = **931 MPa**, which **ASTM B159 independently gives as 135 kpsi for the 0.025–0.0625 in band** — exact two-source agreement.

## Notes
- MH provides allowable design stress as curves (Fig 1–10) + correction factors (Table 1), not clean per-material "% of tensile" scalars; hence torsion sourced from optimumspring and cross-checked against MH, with bending/set as conservative placeholders pending the diameter-dependent design-stress sub-project.
- Cross-validation of method: MH Table 20 reproduces the existing repo's Chrome-Silicon (E=203.4, G=77.2 GPa) and 302 SS (E=193.0, G=68.9 GPa) exactly.
