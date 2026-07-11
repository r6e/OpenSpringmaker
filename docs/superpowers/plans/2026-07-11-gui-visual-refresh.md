# GUI Visual Refresh Implementation Plan (display-polish PR 1 of 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Demo-ready visual consistency — family tab row, shared segmented controls, five-panel canon, spacing tokens, styled/state-aware placeholders, screen shell — plus the Palette pre-staging so PR 2 (light theme) never touches the five family views.

**Architecture:** Everything routes through `widgets.rs` (the shared kit) and `app.rs` (the palette). The `C` const namespace becomes a `Palette` struct (`DARK` only in this PR); views resolve it once via `app.pal()` and pass it down — style fns become palette-parameterized closure factories (the existing `correction_option_style(selected)` precedent). Views stay humble (ADR 0008); all canon changes are view/presenter-level with presenter tests.

**Tech Stack:** Rust, iced 0.14 (`Simulator` in ui_tests), plotters (bitmap pipeline unchanged except a `&Palette` parameter).

## Global Constraints

- Branch: `feat/gui-visual-refresh` (spec f06a64b). springmaker-only — springcore untouched.
- Strict TDD. The existing 443-test floor is the regression net; NO existing assertion may be weakened. Radio-workaround assertions are REPLACED with direct label queries (strengthening).
- ADR 0008: presenters pure (no iced imports); views humble.
- Both clippy commands must pass: `cargo clippy -p springmaker -- -D warnings` AND `cargo clippy -p springmaker --all-targets -- -D warnings`.
- Every commit: `cargo fmt --all` first; conventional message; trailer `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- Section-order canon: hero → "Geometry" rows → "Load points" → toggle + visual → Fatigue → Min-weight → family footer. Assembly keeps "Summary".
- Copy canon (exact strings): `"% MTS"`, `"% Allow"`, hero labels `"Spring rate"` / `"Angular rate"` (torsion), scene placeholder capped-wording `"3D view unavailable: coil count exceeds the renderable 3D limit."`, unchanged `"3D view unavailable for this design (check inputs)."` and `"Chart unavailable for this design (check inputs)."`.
- Spacing canon map: 4→`SP_XS`, 6→`SP_SM_TIGHT` stays 6 (results-column rhythm is load-bearing; see Task 3), 8→`SP_SM`, 10→`SP_MD` (12), 12→`SP_MD`, 16→`SP_LG`, 20→`PANEL_PAD`, 24→`SP_XL`.
- Spec deviation (approved path): style fns take the resolved `&'static Palette` (parameterized-closure precedent) instead of `Palette::of(&Theme)` lookup — Task 1 amends the spec's §Resolution sentence to match.

---

### Task 1: Palette struct + widgets/calculator/settings/materials migration

**Files:**
- Modify: `springmaker/src/app.rs:14-93` (C → Palette + DARK + `App::pal`)
- Modify: `springmaker/src/widgets.rs` (all style fns + helpers)
- Modify: `springmaker/src/calculator.rs`, `springmaker/src/settings_view.rs`, `springmaker/src/materials_view.rs` (call sites)
- Modify: `docs/superpowers/specs/2026-07-11-display-polish-design.md` (§Resolution sentence)
- Test: `springmaker/src/app.rs` tests module

**Interfaces:**
- Produces: `pub struct Palette { pub ink, panel, raised, line, text, muted, accent, warn, danger, success: Color }`; `pub const DARK: Palette` (app.rs, module level); `impl App { pub(crate) fn pal(&self) -> &'static Palette { &DARK } }`.
- Produces: style-fn factories with pal parameter, exact signatures:
  `text_input_style(pal: &'static Palette) -> impl Fn(&Theme, text_input::Status) -> text_input::Style`
  (same shape for `ghost_button_style`, `danger_button_style`, `accent_button_style`, `nav_button_style`; `panel_container(pal, content)`; `styled_pick_list(pal, options, selected, on_select)`; `section_divider(pal)`, `section_heading(pal, label)`, `field_label(pal, label)`, `mono_value(value, color: Color, size)` unchanged (callers now pass `pal.x`); `results_empty(pal)`, `results_error(pal, msg)`, `result_row(pal, …)`, `result_row_colored` unchanged shape + pal, `render_result_row(pal, r)`, `rows_section(pal, heading, rows)`, `divided_result_section(pal, heading, rows)`, `render_governing_rate(pal, label: &str, gr)` (label param used by Task 6), `visual_toggle(pal, selected)` (unchanged radios in this task — Task 4 replaces them), `labeled_input(pal, …)`, `material_picker(app)` / `material_picker_for_member(app, index)` (resolve `app.pal()` internally).
- Later tasks rely on: `DARK`, `App::pal`, and every widgets signature above verbatim.

- [ ] **Step 1: Write the failing test** (app.rs tests module)

```rust
#[test]
fn palette_dark_matches_the_legacy_c_tokens() {
    // Pin the exact legacy values so the C→Palette migration is provably
    // color-identical (any drift = silent restyle).
    assert_eq!(DARK.ink, Color { r: 0.055, g: 0.067, b: 0.086, a: 1.0 });
    assert_eq!(DARK.accent, Color { r: 0.298, g: 0.761, b: 1.0, a: 1.0 });
    assert_eq!(DARK.danger, Color { r: 1.0, g: 0.420, b: 0.420, a: 1.0 });
    assert_eq!(DARK.success, Color { r: 0.31, g: 0.78, b: 0.47, a: 1.0 });
}
```

- [ ] **Step 2: Run** `cargo test -p springmaker palette_dark_matches` — FAIL (DARK undefined).

- [ ] **Step 3: Implement Palette in app.rs** — replace `pub struct C; impl C { … }` (app.rs:19-93) with:

```rust
/// One resolved color palette. PR 2 adds `LIGHT`; views resolve the active
/// palette once per view build via [`App::pal`] and pass it down — theme
/// switches re-run `view()`, so build-time resolution stays correct.
pub struct Palette {
    pub ink: Color,
    pub panel: Color,
    pub raised: Color,
    pub line: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub warn: Color,
    pub danger: Color,
    pub success: Color,
}

/// The engineering-instrument dark palette (the shipped identity).
pub const DARK: Palette = Palette {
    ink: Color { r: 0.055, g: 0.067, b: 0.086, a: 1.0 },
    panel: Color { r: 0.090, g: 0.110, b: 0.141, a: 1.0 },
    raised: Color { r: 0.122, g: 0.149, b: 0.188, a: 1.0 },
    line: Color { r: 0.165, g: 0.196, b: 0.239, a: 1.0 },
    text: Color { r: 0.902, g: 0.918, b: 0.941, a: 1.0 },
    muted: Color { r: 0.541, g: 0.592, b: 0.651, a: 1.0 },
    accent: Color { r: 0.298, g: 0.761, b: 1.0, a: 1.0 },
    warn: Color { r: 0.949, g: 0.710, b: 0.227, a: 1.0 },
    danger: Color { r: 1.0, g: 0.420, b: 0.420, a: 1.0 },
    success: Color { r: 0.31, g: 0.78, b: 0.47, a: 1.0 },
};
```

and add to `impl App`:

```rust
/// The active palette. PR 2 resolves Light/Dark/System here; today it is
/// always the dark identity.
pub(crate) fn pal(&self) -> &'static Palette {
    &DARK
}
```

Doc comments carry each token's role (ink = app background, etc.) exactly as the old consts did.

- [ ] **Step 4: Migrate widgets.rs** — every fn per the Interfaces block. Representative transforms (apply the same shape to every listed fn — the mapping `C::X` → `pal.x` is total):

```rust
pub(crate) fn panel_container<'a>(
    pal: &'static Palette,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(content)
        .padding(20)
        .style(move |_theme| iced::widget::container::Style {
            background: Some(Background::Color(pal.panel)),
            border: Border { color: pal.line, width: 1.0, radius: 6.0.into() },
            ..Default::default()
        })
        .into()
}

pub(crate) fn text_input_style(
    pal: &'static Palette,
) -> impl Fn(&iced::Theme, iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    move |_theme, status| {
        use iced::widget::text_input::Status;
        let focused = matches!(status, Status::Focused { .. });
        iced::widget::text_input::Style {
            background: Background::Color(pal.raised),
            border: Border {
                color: if focused { pal.accent } else { pal.line },
                width: if focused { 1.5 } else { 1.0 },
                radius: 4.0.into(),
            },
            icon: pal.muted,
            placeholder: pal.muted,
            value: pal.text,
            selection: Color { a: 0.3, ..pal.accent },
        }
    }
}

pub(crate) fn render_governing_rate(
    pal: &'static Palette,
    label: &str,
    gr: &GoverningRate,
) -> Element<'static, Message> {
    let rate_label = text(label.to_owned()).size(SZ_LABEL).color(pal.muted);
    let rate_value = mono_value(format!("{} {}", gr.value, gr.unit), pal.accent, SZ_HERO);
    column![rate_label, rate_value].spacing(6).into()
}
```

Every other fn follows identically (`.style(move |_theme, status| …)` capturing `pal`). `material_picker`/`material_picker_for_member` start with `let pal = app.pal();`. Remove `use crate::app::C` imports, import `Palette`/`DARK` as needed.

- [ ] **Step 5: Migrate calculator.rs / settings_view.rs / materials_view.rs** — each view fn opens with `let pal = app.pal();`; every `C::X` → `pal.x`; every `.style(some_style)` → `.style(some_style(pal))`; `correction_option_style(selected)` → `correction_option_style(pal, selected)` (add the param, same body with `pal.x`). `render_status_line(line)` → `render_status_line(pal, line)`; `footer()` → `footer(pal)`. All existing hero call sites become `render_governing_rate(pal, "Spring rate", &p.governing_rate)` — compression/extension/conical/assembly (this compiles only after Step 6's family sweep; do Steps 4-6 as one unit).

- [ ] **Step 6: Sweep the five family view files + remaining C:: users** for compilation: `grep -rn "C::" springmaker/src/` must reach ZERO production hits (test modules may pin colors via `DARK.x`). Family views open with `let pal = app.pal();` and pass `pal` through to every widgets call. `plot/canvas.rs`, `plot/render.rs`, `viz/canvas3d.rs`, `viz/render3d.rs` keep compiling via `use crate::app::DARK` TEMPORARILY (Task 2 threads the parameter) — mark each with `// Task 2 threads &Palette here`.

- [ ] **Step 7: Amend the spec** §Resolution first sentence to: "Style fns are palette-parameterized closure factories (the `correction_option_style` precedent); views resolve `app.pal()` once per build — theme switches re-run `view()`, so no `&Theme` lookup is needed."

- [ ] **Step 8: Run** `cargo test -p springmaker` — 443+1 pass. Both clippy commands clean.

- [ ] **Step 9: Commit** `refactor(gui): replace the C const namespace with the Palette struct (PR-2 pre-staging)`

### Task 2: Thread &Palette through the bitmap pipeline

**Files:**
- Modify: `springmaker/src/plot/mod.rs`, `plot/render.rs`, `plot/canvas.rs`, `viz/render3d.rs`, `viz/canvas3d.rs`
- Modify: the five family `view.rs` visual-slot call sites
- Test: `springmaker/src/plot/render.rs`, `springmaker/src/viz/render3d.rs` tests

**Interfaces:**
- Produces: `pub fn chart_element(pal: &'static Palette, data: ChartData) -> Element<'static, Message>`; `pub fn scene_element(pal: &'static Palette, scene: SceneData, orbit: Orbit) -> Element<'static, Message>`; `pub fn render_chart(pal: &Palette, data: &ChartData) -> Option<(Vec<u8>, ChartMapping)>`; `pub fn render_scene(pal: &Palette, scene: &SceneData, orbit: Orbit) -> Option<Vec<u8>>`; `role_color(pal: &Palette, role: SceneRole) -> RGBColor`; `line_style(pal: &Palette, role: LineRole)`, `marker_style(pal: &Palette, kind: MarkerKind)`.
- Consumes: Task 1's `Palette`/`DARK`/`app.pal()`.

- [ ] **Step 1: Write the failing test** (plot/render.rs tests)

```rust
#[test]
fn render_chart_background_follows_the_palette() {
    // Same data, two palettes differing only in panel color ⇒ different
    // corner pixel. Pins that the renderer reads the parameter, not DARK.
    let alt = Palette { panel: Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }, ..DARK };
    let (dark_px, _) = render_chart(&DARK, &simple_data(false)).unwrap();
    let (alt_px, _) = render_chart(&alt, &simple_data(false)).unwrap();
    assert_ne!(dark_px[0..3], alt_px[0..3], "corner pixel must follow pal.panel");
}
```

(`Palette` must be `Clone`-free const-constructible — struct-update syntax on a const works because all fields are `Copy`.) Mirror test `render_scene_background_follows_the_palette` in viz/render3d.rs with the same two palettes.

- [ ] **Step 2: Run both** — FAIL (signatures don't take pal).

- [ ] **Step 3: Implement** — add the `pal` parameter per Interfaces; inside, every `to_rgb(C::X)`/`to_rgb(DARK.x)` becomes `to_rgb(pal.x)`; `ensure_font` untouched. `chart_element`/`scene_element` pass through to the renderers and use `pal.muted` for nothing yet (placeholder styling is Task 8). Five family views: `crate::plot::chart_element(pal, …)`, `crate::viz::scene_element(pal, …, app.orbit)`, `fatigue_chart_data(outcome, us).map(|d| crate::plot::chart_element(pal, d))`. Delete the Task-2 marker comments from Task 1 Step 6.

- [ ] **Step 4: Run** `cargo test -p springmaker` — all pass (buffer-equality and placeholder pins included). Both clippy clean. `grep -rn "DARK" springmaker/src/plot springmaker/src/viz` shows test-module hits only.

- [ ] **Step 5: Commit** `refactor(gui): thread the palette through the chart and scene renderers`

### Task 3: Spacing tokens

**Files:**
- Modify: `springmaker/src/widgets.rs` (constants), all `.spacing(`/`.padding(` literal sites in `springmaker/src/{calculator.rs,settings_view.rs,materials_view.rs,widgets.rs}` and the five family `view.rs`
- Test: none new (layout is unpinned); gate = suite green + grep sweep

**Interfaces:**
- Produces (widgets.rs, by SZ_* consts): `pub(crate) const SP_XS: u16 = 4; SP_ROW: u16 = 6; SP_SM: u16 = 8; SP_MD: u16 = 12; SP_LG: u16 = 16; SP_XL: u16 = 24; PANEL_PAD: u16 = 20; COL_PT: f32 = 24.0; COL_STATUS_PREFIX: f32 = 72.0; HEADER_GAP: f32 = 160.0;`

- [ ] **Step 1: Add the constants** with doc comments (SP_ROW=6 keeps the existing results-row rhythm — a deliberate token, not a rounding; 10 maps UP to SP_MD per the canon map in Global Constraints).

- [ ] **Step 2: Migrate literals** — every `.spacing(N)`/`.padding(N)` in the listed files to the token (`.spacing(SP_LG)` etc.); `Length::Fixed(24.0)` "Pt" columns → `Length::Fixed(COL_PT)`; status prefix 72 → `COL_STATUS_PREFIX`; header spacer 160 → `HEADER_GAP`; panel padding 20 → `PANEL_PAD`; screen-root padding 24 → `SP_XL`. The `[8, 12]` settings button padding → `[SP_SM, SP_MD]`. Verify: `grep -rnE '\.spacing\([0-9]|\.padding\([0-9]' springmaker/src --include=*.rs | grep -v test` → zero hits.

- [ ] **Step 3: Run** `cargo test -p springmaker` (all pass — only the two 10→12 sites change rendered output, nothing pins them), both clippy.

- [ ] **Step 4: Commit** `refactor(gui): name the spacing scale (SP_* tokens)`

### Task 4: Shared segmented control replaces every radio cluster

**Files:**
- Modify: `springmaker/src/widgets.rs` (new `segmented`, `segmented_style`; rewrite `visual_toggle`)
- Modify: `springmaker/src/calculator.rs` (units), `springmaker/src/extension/view.rs:73-99` (hook mode), `springmaker/src/settings_view.rs` (options through `segmented`)
- Test: `springmaker/src/ui_tests.rs`

**Interfaces:**
- Produces:

```rust
/// One-of-N chooser rendered as a row of styled buttons (labels are real
/// text() children, so the Simulator can find and click them — iced radio
/// labels are structurally invisible to it).
pub(crate) fn segmented<'a, T: PartialEq + Copy + 'a>(
    pal: &'static Palette,
    options: &[(&'static str, T)],
    selected: T,
    on_pick: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message>
```

- Produces: `segmented_style(pal, selected: bool)` — exact body = today's `correction_option_style` moved from settings_view.rs into widgets.rs (with `pal.x` for `C::X`); settings imports it back.
- `visual_toggle(pal, selected)` becomes `segmented(pal, &[("Chart", VisualMode::Chart), ("3D", VisualMode::Spring3d)], selected, Message::Visual)`.

- [ ] **Step 1: Write the failing tests** (ui_tests.rs)

```rust
#[test]
fn units_toggle_switches_by_clicking_the_label() {
    let mut app = probe_solve_compression();
    let mut ui = simulator(&app);
    let _ = ui.click("US (in, lbf)").expect("units label must be clickable");
    for m in ui.into_messages() { app.update(m); }
    assert_eq!(app.unit_system, UnitSystem::Us);
}

#[test]
fn visual_toggle_switches_by_clicking_the_label() {
    let mut app = probe_solve_compression();
    let mut ui = simulator(&app);
    let _ = ui.click("3D").expect("3D label must be clickable");
    for m in ui.into_messages() { app.update(m); }
    assert_eq!(app.results_visual, VisualMode::Spring3d);
    assert!(!shows(&app, CHART_PLACEHOLDER) && !shows(&app, SCENE_PLACEHOLDER));
}

#[test]
fn hook_mode_switches_by_clicking_the_label() {
    let mut app = probe_solve_extension();
    let mut ui = simulator(&app);
    let _ = ui.click("Custom radii").expect("hook-mode label must be clickable");
    for m in ui.into_messages() { app.update(m); }
    assert_eq!(app.extension.hook_mode, HookMode::Custom);
}
```

(Match the file's existing simulator/click idiom exactly — read neighboring tests like the settings correction-click test first; `simulator`/`click`/`into_messages` names must follow that precedent, not this sketch.)

- [ ] **Step 2: Run** — FAIL: `click` cannot find radio labels (`Candidate::Text` never fed).

- [ ] **Step 3: Implement** `segmented` + `segmented_style` in widgets.rs:

```rust
pub(crate) fn segmented_style(
    pal: &'static Palette,
    selected: bool,
) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, status| {
        let is_hovered = matches!(status, iced::widget::button::Status::Hovered);
        let bg = if selected {
            Color { r: pal.accent.r * 0.15, g: pal.accent.g * 0.15, b: pal.accent.b * 0.15, a: 1.0 }
        } else if is_hovered {
            Color { r: pal.raised.r + 0.05, g: pal.raised.g + 0.05, b: pal.raised.b + 0.05, a: 1.0 }
        } else {
            Color::TRANSPARENT
        };
        iced::widget::button::Style {
            background: Some(Background::Color(bg)),
            text_color: if selected { pal.accent } else { pal.text },
            border: Border {
                color: if selected { pal.accent } else { pal.line },
                width: 1.0,
                radius: 4.0.into(),
            },
            shadow: Default::default(),
            snap: Default::default(),
        }
    }
}

pub(crate) fn segmented<'a, T: PartialEq + Copy + 'a>(
    pal: &'static Palette,
    options: &[(&'static str, T)],
    selected: T,
    on_pick: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message> {
    let mut r = row![].spacing(SP_XS);
    for (label, value) in options {
        r = r.push(
            button(text(*label).size(SZ_LABEL))
                .on_press(on_pick(*value))
                .style(segmented_style(pal, *value == selected))
                .padding([SP_XS, SP_MD]),
        );
    }
    r.into()
}
```

Replace the three radio clusters (units in calculator header, hook mode in extension, `visual_toggle` body); settings' option loop becomes a `segmented`-per-row equivalent ONLY if its full-width layout survives — otherwise settings keeps its buttons but styles them via the now-shared `segmented_style` (the dedup is the requirement, not the widget). Remove `radio` imports everywhere; `grep -rn "radio" springmaker/src --include=*.rs` → zero production hits.

- [ ] **Step 4: Replace the radio-workaround assertions** — in ui_tests.rs, find every comment/assertion working around unqueryable radio labels (the `visual_toggle_swaps_chart_for_3d` comment block from the 3D increment is the anchor: it documents WHY "Chart"/"3D" could not be asserted). Add the direct assertions those tests could not make (e.g. `assert!(shows(&app, "Chart") && shows(&app, "3D"))` in the toggle test) and delete the now-false workaround comments. Do NOT remove any existing assertion.

- [ ] **Step 5: Run** `cargo test -p springmaker` — all green. Both clippy.

- [ ] **Step 6: Commit** `feat(gui): shared segmented control — units, chart/3D, hook mode, settings`

### Task 5: Family tab row

**Files:**
- Modify: `springmaker/src/calculator.rs:75-128` (header)
- Test: `springmaker/src/ui_tests.rs`

**Interfaces:**
- Consumes: `segmented` (Task 4), `Message::SelectFamily`, `ALL_FAMILIES`.
- Produces: header layout = app name / tab row / spacer / nav buttons / units control; the pick_list family selector is GONE (styled_pick_list itself stays — material pickers use it).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn family_tab_row_switches_family_by_clicking_the_label() {
    let mut app = test_app();
    let mut ui = simulator(&app);
    let _ = ui.click("Torsion").expect("family tab must be clickable");
    for m in ui.into_messages() { app.update(m); }
    assert_eq!(app.family, Family::Torsion);
    // All five tabs visible at once — the demo-breadth requirement.
    for name in ["Compression", "Extension", "Torsion", "Conical", "Assembly"] {
        assert!(shows(&app, name), "tab {name} must be visible");
    }
}
```

- [ ] **Step 2: Run** — FAIL (pick_list menu labels are not visible text until opened).

- [ ] **Step 3: Implement** — in `header`, replace the `family_selector` container with:

```rust
let family_tabs = segmented(
    pal,
    &[
        ("Compression", Family::Compression),
        ("Extension", Family::Extension),
        ("Torsion", Family::Torsion),
        ("Conical", Family::Conical),
        ("Assembly", Family::Assembly),
    ],
    app.family,
    Message::SelectFamily,
);
```

Header row becomes `row![app_name, space().width(Length::Fixed(HEADER_GAP)), family_tabs, space().width(Length::Fill), materials_btn, settings_btn, units_control].spacing(SP_LG)`. Drop the `styled_pick_list`/`ALL_FAMILIES` imports from calculator.rs if now unused. NOTE: if the header overflows 1200px with all five tabs + controls (check by eye via `cargo run` if available, else accept), move the units control down beside the footer buttons — decide, implement one, and record which in the report.

- [ ] **Step 4: Run** full suite; existing family-switch tests (which dispatch `Message::SelectFamily` directly) stay green. Both clippy.

- [ ] **Step 5: Commit** `feat(gui): family tab row replaces the header dropdown`

### Task 6: Canon — torsion hero + section order + conditional pushes

**Files:**
- Modify: `springmaker/src/torsion/view_model.rs` (add `governing_rate`), `springmaker/src/torsion/view.rs:283-360`, `springmaker/src/compression/view.rs:246-343`
- Test: `springmaker/src/torsion/view_model.rs` tests, `springmaker/src/ui_tests.rs`

**Interfaces:**
- Consumes: `render_governing_rate(pal, label, gr)` (Task 1), `GoverningRate { value, unit }`.
- Produces: `TorPopulatedResults.governing_rate: GoverningRate` (from the solved angular rate, same formatted value as the existing `rate_per_deg` row); torsion populated order = hero("Angular rate") → Geometry rows (the two rate rows fold into the top of `p.geometry`? NO — keep `rate_per_turn` as the first Geometry-section row and delete `rate_per_deg` as a row since the hero now carries it).

- [ ] **Step 1: Write the failing presenter test** (torsion/view_model.rs)

```rust
#[test]
fn tor_populated_carries_a_governing_rate_hero() {
    let p = populated_fixture(); // the module's existing populated helper
    assert_eq!(p.governing_rate.value, p_rate_per_deg_value_of_the_same_fixture());
    // The per-degree row no longer duplicates the hero; per-turn leads Geometry.
    assert_eq!(p.geometry[0].label, "Rate (per turn)");
}
```

(Adapt names to the module's real fixture helpers and the real `rate_per_turn` label — read the module's existing tests first; the assertions' CONTENT is the requirement.)

- [ ] **Step 2: Run** — FAIL (no `governing_rate` field).

- [ ] **Step 3: Implement** — in `TorPopulatedResults` add `pub governing_rate: GoverningRate`; construct it where `rate_per_deg` is built today (same formatted value + unit string); remove `rate_per_deg` from the struct and prepend the `rate_per_turn` row to `geometry` (delete the standalone `rate_per_turn` field). Update torsion/view.rs populated arm:

```rust
let mut col = column![
    section_heading(pal, "Results"),
    section_divider(pal),
    render_governing_rate(pal, "Angular rate", &p.governing_rate),
    section_divider(pal),
    rows_section(pal, "Geometry", &p.geometry),
    section_divider(pal),
    render_tor_load_table(pal, &p.load_table),
    section_divider(pal),
    toggle,
    visual,
]
.spacing(SP_ROW);

match &p.fatigue {
    TorFatigueView::Hidden => {}
    TorFatigueView::Computed(rows) => {
        col = col.push(divided_result_section(pal, "Fatigue analysis", rows));
    }
    TorFatigueView::Note(msg) => {
        col = col.push(
            column![section_divider(pal), text(*msg).size(SZ_LABEL).color(pal.muted)]
                .spacing(SP_SM),
        );
    }
}
if let Some(fc) = fatigue_chart {
    col = col.push(fc);
}
if let Some(rows) = &p.min_weight {
    col = col.push(divided_result_section(pal, "Min-weight optimisation", rows));
}
```

(Fatigue now precedes Min-weight — the canon.) Compression's `render_populated` switches to conditional pushes:

```rust
match &p.fatigue {
    FatigueView::Hidden => {}
    FatigueView::Computed(rows) => {
        col = col.push(divided_result_section(pal, "Fatigue analysis", rows));
    }
    FatigueView::Note(msg) => {
        col = col.push(
            column![section_divider(pal), text(*msg).size(SZ_LABEL).color(pal.muted)]
                .spacing(SP_SM),
        );
    }
}
if let Some(fc) = fatigue_chart {
    col = col.push(fc);
}
if let MinWeightView::Shown(rows) = &p.min_weight {
    col = col.push(divided_result_section(pal, "Min-weight optimisation", rows));
}
```

Delete compression's now-unused `render_fatigue`/`render_min_weight` helpers. Fix every torsion view_model test that pinned `rate_per_deg`/`rate_per_turn` struct fields to the new shape (content assertions keep their values — only the location moves).

- [ ] **Step 4: Add the ui_test pin**

```rust
#[test]
fn torsion_shows_the_angular_rate_hero() {
    let app = probe_solve_torsion();
    assert!(shows(&app, "Angular rate"));
}
```

- [ ] **Step 5: Run** full suite. Both clippy. Commit `feat(gui): torsion hero rate + canonical section order + conditional sections`

### Task 7: Emphasis promotion + spaced % headers

**Files:**
- Modify: `springmaker/src/presenter.rs:56-72` (LoadRow), `springmaker/src/{compression,conical,assembly}/view_model.rs` (Danger rule), `springmaker/src/extension/view_model.rs` (3 emphasis fields), the four table renderers in `{compression,conical,extension,assembly}/view.rs` (+ header strings)
- Test: each view_model tests module

**Interfaces:**
- Produces: `LoadRow.stress_emphasis: Emphasis` (presenter.rs); extension's row struct gains `body_emphasis`, `bending_emphasis`, `torsion_emphasis: Emphasis`.
- Danger rule (torsion's precedent): emphasis = `Emphasis::Danger` iff the corresponding engine fraction `> 1.0` (e.g. `lp.pct_mts > 1.0`), else Normal.
- Header canon: `"% MTS"` (compression/conical/assembly-member), extension's three percent headers keep their symbols but gain the space (`"% τ_body"` form), torsion already `"% Allow"`.

- [ ] **Step 1: Write the failing presenter tests** — one per family, at an overstressed fixture (each view_model already has an overstress/huge-load fixture; reuse it):

```rust
#[test]
fn overstressed_load_point_carries_danger_emphasis() {
    let p = populated_overstressed_fixture();
    assert_eq!(p.load_table.rows[0].stress_emphasis, Emphasis::Danger);
}

#[test]
fn normal_load_point_carries_normal_emphasis() {
    let p = populated_fixture();
    assert_eq!(p.load_table.rows[0].stress_emphasis, Emphasis::Normal);
}
```

(Extension: assert all three fields at a body-overstressed fixture — each column's emphasis follows ITS OWN fraction, pin at least one mixed case where body is Danger while bending is Normal.)

- [ ] **Step 2: Run** — FAIL (fields missing).

- [ ] **Step 3: Implement** — presenter.rs LoadRow gains the field; every constructor site sets it (`if lp.pct_mts > 1.0 { Emphasis::Danger } else { Emphasis::Normal }`); assembly's empty-cell assembly-level rows set `Emphasis::Normal`. Table renderers color the stress + percent cells exactly as torsion's does (torsion/view.rs:232-264 is the template — `let stress_color = match … { Normal => pal.text, Danger => pal.danger }` applied to both cells). Header strings updated to the spaced canon.

- [ ] **Step 4: Run** full suite — fix any test pinning the old `"%MTS"` header string (content updates to `"% MTS"`, never deletions). Both clippy.

- [ ] **Step 5: Commit** `feat(gui): overstress emphasis in every load table + spaced percent headers`

### Task 8: Placeholders, member cards, screen shell, letterbox helper

**Files:**
- Modify: `springmaker/src/plot/canvas.rs`, `springmaker/src/viz/canvas3d.rs`, `springmaker/src/plot/mapping.rs` (letterbox helper home), `springmaker/src/assembly/view.rs` (member cards), `springmaker/src/widgets.rs` (screen_shell), `springmaker/src/{calculator,materials_view,settings_view}.rs` (shell adoption)
- Test: `springmaker/src/ui_tests.rs`, `springmaker/src/viz/canvas3d.rs` tests

**Interfaces:**
- Produces: `pub(crate) fn placeholder_text(pal: &'static Palette, msg: &str) -> Element<'static, Message>` (widgets.rs — `SZ_BODY`, `pal.muted`); `pub(crate) const SCENE_PLACEHOLDER_CAPPED: &str = "3D view unavailable: coil count exceeds the renderable 3D limit.";` (canvas3d.rs beside SCENE_PLACEHOLDER); `pub(crate) fn draw_letterboxed_bitmap(frame: &mut Frame, lb: &Letterbox, handle: &image::Handle)` (mapping.rs, used by both canvases); `pub(crate) fn screen_shell<'a>(pal, content: impl Into<Element<'a, Message>>) -> Element<'a, Message>` (widgets.rs: scrollable → padding SP_XL → max_width 1200 → ink background, replacing the three root-chrome copies); assembly `render_member_section` wrapped in a `raised`-bg bordered container (radius 4, padding SP_SM).

- [ ] **Step 1: Write the failing tests**

```rust
// ui_tests.rs — capped coil count is VALID input; the wording must say so.
#[test]
fn capped_torsion_names_the_render_limit_not_bad_inputs() {
    let mut app = probe_solve_torsion_with_body_coils("2001");
    app.update(Message::Visual(VisualMode::Spring3d));
    assert!(shows(&app, "renderable 3D limit"));
    assert!(!shows(&app, "check inputs"));
}

// canvas3d.rs tests — the split is data-driven: empty body ⇒ capped wording.
#[test]
fn scene_element_picks_the_capped_wording_for_an_empty_body() {
    let scene = close_wound_coil(10.0, 2001.0, 2.0); // capped ⇒ empty body
    assert!(coil_body_is_empty(&scene)); // sanity: the discriminator
    // scene_element returns the capped placeholder element; pin via the
    // existing element-text extraction idiom in this module (or assert the
    // chooser fn directly if extraction is impractical):
    assert_eq!(placeholder_for(&scene), SCENE_PLACEHOLDER_CAPPED);
}

#[test]
fn scene_element_keeps_check_inputs_for_nonfinite_geometry() {
    let scene = scene_with_nan_points(); // build inline: one polyline, NaN y
    assert_eq!(placeholder_for(&scene), SCENE_PLACEHOLDER);
}
```

Implement `fn placeholder_for(scene: &SceneData) -> &'static str` as the pure chooser `scene_element` calls: `if coil_body_is_empty(scene) { SCENE_PLACEHOLDER_CAPPED } else { SCENE_PLACEHOLDER }` — evaluated only on the `render_scene == None` path (a capped assembly returns zero polylines, which `coil_body_is_empty` already reports true — one wording for both).

- [ ] **Step 2: Run** — FAIL. **Step 3: Implement** all Interfaces items: both `None` arms become `placeholder_text(pal, …)`; the three screen roots collapse onto `screen_shell` (materials keeps its inner per-panel scrollables — shell wraps WITHOUT the outer scrollable for materials: give `screen_shell` a `scroll: bool` parameter, calculator/settings pass true, materials false); `OrbitCanvas::draw`/`ChartCanvas::draw` letterbox blocks collapse onto `draw_letterboxed_bitmap`; assembly member sections get the sub-card container. Settings' `max_width(800)` becomes the shell's 1200.

- [ ] **Step 4: Run** full suite (placeholder pins updated where the wording legitimately changed — the capped tests from the 3D increment asserted `SCENE_PLACEHOLDER`; they now assert `SCENE_PLACEHOLDER_CAPPED` — content change, not weakening; every other placeholder pin untouched). Both clippy.

- [ ] **Step 5: Commit** `feat(gui): state-aware placeholders, member cards, shared screen shell`

### Task 9: Carried test items + full gate

**Files:**
- Modify: `springmaker/src/ui_tests.rs` (five-family split), `springmaker/src/assembly/scene_model.rs` (comment nit)
- Test: this task IS tests

- [ ] **Step 1: Split** `every_family_renders_3d_after_solve` into five per-family tests (`compression_renders_3d_after_solve` etc.), each the existing per-family block verbatim + its populated-proof; delete the bundled test. Use the `probe_solve_*` helpers where they exist.

- [ ] **Step 2: Fix the comment** in assembly/scene_model.rs (the NaN-cascade block): the sentence claiming "a NaN-poisoned member still contributes points" is now imprecise — NaN COIL COUNTS route through the `scene_from_radius` entry guard (empty body → whole-scene bail); only NaN in non-coil fields (pitch/diameter) produces the contributing-points cascade. Reword to name both paths.

- [ ] **Step 3: Full gate** — `cargo fmt --all --check`; `cargo test --workspace`; BOTH clippy; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`; `typos`. Record counts.

- [ ] **Step 4: Commit** `test(gui): per-family 3D pins + assembly cascade comment precision`

---

## Self-review notes (already applied)

- Spec coverage: tab row (T5), segmented (T4), canon (T6/T7), tokens (T3), placeholders/cards/shell (T8), carried items (T8/T9), pre-staging (T1/T2 incl. renderer params + spec §Resolution amendment). Compression two-column setup: explicitly untouched (spec canon).
- Type consistency: `pal: &'static Palette` everywhere; `render_governing_rate(pal, label, gr)` defined in T1, consumed in T6; `segmented` defined in T4, consumed in T5; `SP_*` defined in T3, consumed in T4-T8 (T1/T2 land before T3 — they keep literal spacings and T3 sweeps them).
- Known judgment points left to implementers ON PURPOSE (each must be recorded in the task report): settings segmented-vs-styled-buttons layout call (T4 Step 3), header overflow fallback (T5 Step 3), simulator idiom names (T4/T5/T6 test sketches follow the file's real helpers).
