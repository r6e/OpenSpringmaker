# GUI Light Theme Implementation Plan (display-polish PR 2 of 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Light/Dark/System theme — `Palette::LIGHT` (WCAG-AA-tested), a Settings picker (default System) with live OS-theme tracking, persisted alongside the curvature correction.

**Architecture:** PR 1 pre-staged everything: style fns are palette factories, views resolve `app.pal()` per build, renderers take `&Palette`. This PR adds the second palette, makes `pal()` resolve `theme_pref × system_mode`, seeds/tracks the OS mode via `iced::system::theme()`/`theme_changes()` (verified un-feature-gated in iced 0.14), and rewires `App::theme()` off its last `DARK` bake-in. The five family views are untouched (spec §PR 2 promise).

**Tech Stack:** Rust, iced 0.14 (`theme::Mode`, `system::theme`/`theme_changes`, application `.subscription`), serde/toml settings store, `snapshot_hash` differential pins.

## Global Constraints

- Branch `feat/gui-light-theme` (off main 0d7195b). springmaker-only; the five family view/view_model files must show ZERO diff.
- Strict TDD; both clippy commands (`cargo clippy -p springmaker -- -D warnings` AND `--all-targets`); fmt/doc/typos gates; commit trailer `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- DARK behavior identity: every existing rendered dark pixel unchanged (derived-field values bit-equal to the old runtime math; the palette-identity test extends to pin them).
- Settings copy (exact): heading `"Theme"`, option labels `"System"`, `"Light"`, `"Dark"`; theme picker sits below the correction panel.
- Default pref System; `Mode::None` (OS reports nothing) resolves to DARK — the shipped identity; documented on the resolution fn.
- Carried panel flags (all land here): derived colors become palette-OWNED fields (no runtime color math in style fns); `App::theme()` reads the resolved palette (bake-in gone); settings pressability decision moves into `SettingsViewModel` as a `clickable` flag (module-doc claim becomes true); mode switching pinned via `snapshot_hash` differentials.

---

### Task 1: Palette-owned derived colors (dark-identical refactor)

**Files:**
- Modify: `springmaker/src/app.rs` (Palette struct + DARK + identity test)
- Modify: `springmaker/src/widgets.rs` (`segmented_style`)
- Modify: `springmaker/src/settings_view.rs` (if it re-derives — check; the style is shared from widgets)

**Interfaces:**
- Produces: `Palette` gains `pub accent_tint: Color` (selected-option background) and `pub hover: Color` (hovered-option background). `segmented_style` reads `pal.accent_tint` / `pal.hover` instead of computing `accent×0.15` / `raised+0.05`.
- DARK's new field values are the EXACT products of the old math (bit-equal f32):
  `accent_tint: Color { r: 0.298 * 0.15, g: 0.761 * 0.15, b: 1.0 * 0.15, a: 1.0 }`,
  `hover: Color { r: 0.122 + 0.05, g: 0.149 + 0.05, b: 0.188 + 0.05, a: 1.0 }`
  (write the expressions, not decimal literals — const arithmetic keeps them bit-identical to what the closure computed).

- [ ] **Step 1: Extend the identity test** (app.rs tests) — RED first:

```rust
#[test]
fn palette_dark_derived_fields_match_the_legacy_runtime_math() {
    assert_eq!(
        DARK.accent_tint,
        Color { r: DARK.accent.r * 0.15, g: DARK.accent.g * 0.15, b: DARK.accent.b * 0.15, a: 1.0 }
    );
    assert_eq!(
        DARK.hover,
        Color { r: DARK.raised.r + 0.05, g: DARK.raised.g + 0.05, b: DARK.raised.b + 0.05, a: 1.0 }
    );
}
```

- [ ] **Step 2: Run** `cargo test -p springmaker palette_dark_derived` — FAIL (fields missing).
- [ ] **Step 3: Implement** — add the two fields (doc comments: "Selected-option background tint. Palette-owned: a ×0.15 dark-tint is a dark-theme assumption; LIGHT defines its own pale tint." / "Hovered-option background. Palette-owned: +0.05 lightens on dark; LIGHT darkens instead."); DARK uses the const expressions above. Rewrite `segmented_style`'s two computed colors to `pal.accent_tint` / `pal.hover`. Grep for any other `* 0.15` / `+ 0.05` color math (`grep -n "0.15\|+ 0.05" springmaker/src/widgets.rs springmaker/src/settings_view.rs`) — the settings loop shares `segmented_style`, so there should be none left; `accent_button_style`'s `×0.85` hover-darken and the text-input selection alpha stay (direction-safe on both palettes — note this adjudication in a comment on each).
- [ ] **Step 4: Run** full `cargo test -p springmaker` (all green — the segmented snapshot pin proves pixel identity) + both clippy.
- [ ] **Step 5: Commit** `refactor(gui): palette-owned derived colors (dark-identical)`

### Task 2: Palette::LIGHT + machine-checked WCAG contrast

**Files:**
- Modify: `springmaker/src/app.rs` (LIGHT const + contrast tests)

**Interfaces:**
- Produces: `pub const LIGHT: Palette` — warm-paper light palette. Starting values (the CONTRAST TEST is the gate; tune values until it passes, keeping the hue intent):

```rust
/// The paper-white light palette — the dark theme's mirror, not an inversion.
pub const LIGHT: Palette = Palette {
    ink: Color { r: 0.965, g: 0.960, b: 0.950, a: 1.0 },
    panel: Color { r: 0.925, g: 0.920, b: 0.908, a: 1.0 },
    raised: Color { r: 0.885, g: 0.880, b: 0.868, a: 1.0 },
    line: Color { r: 0.780, g: 0.775, b: 0.760, a: 1.0 },
    text: Color { r: 0.100, g: 0.110, b: 0.130, a: 1.0 },
    muted: Color { r: 0.320, g: 0.340, b: 0.380, a: 1.0 },
    accent: Color { r: 0.000, g: 0.350, b: 0.620, a: 1.0 },
    warn: Color { r: 0.550, g: 0.360, b: 0.000, a: 1.0 },
    danger: Color { r: 0.780, g: 0.100, b: 0.100, a: 1.0 },
    success: Color { r: 0.050, g: 0.450, b: 0.220, a: 1.0 },
    accent_tint: Color { r: 0.850, g: 0.910, b: 0.970, a: 1.0 },
    hover: Color { r: 0.885 - 0.05, g: 0.880 - 0.05, b: 0.868 - 0.05, a: 1.0 },
};
```

- [ ] **Step 1: Write the contrast test** (app.rs tests) — RED first (LIGHT undefined). WCAG 2.x relative luminance + contrast ratio, in-test helpers:

```rust
fn srgb_lin(c: f32) -> f64 {
    let c = c as f64;
    if c <= 0.040_45 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}
fn luminance(c: Color) -> f64 {
    0.2126 * srgb_lin(c.r) + 0.7152 * srgb_lin(c.g) + 0.0722 * srgb_lin(c.b)
}
fn contrast(a: Color, b: Color) -> f64 {
    let (l1, l2) = (luminance(a).max(luminance(b)), luminance(a).min(luminance(b)));
    (l1 + 0.05) / (l2 + 0.05)
}

#[test]
fn light_palette_meets_wcag_aa_on_both_surfaces() {
    // Body text sizes here are 11-14px — AA small-text threshold 4.5:1.
    for bg in [LIGHT.ink, LIGHT.panel, LIGHT.raised] {
        for fg in [LIGHT.text, LIGHT.muted, LIGHT.accent, LIGHT.danger, LIGHT.warn, LIGHT.success] {
            assert!(
                contrast(fg, bg) >= 4.5,
                "LIGHT fg {fg:?} on bg {bg:?} = {:.2}, needs 4.5", contrast(fg, bg)
            );
        }
    }
    // Selected-option text is accent-on-accent_tint (segmented_style).
    assert!(contrast(LIGHT.accent, LIGHT.accent_tint) >= 4.5);
    // Structural sanity: light surfaces order light→dark as ink ≥ panel ≥ raised > hover.
    assert!(luminance(LIGHT.ink) > luminance(LIGHT.panel));
    assert!(luminance(LIGHT.panel) > luminance(LIGHT.raised));
    assert!(luminance(LIGHT.raised) > luminance(LIGHT.hover));
    // The hairline must remain visible but not text-strong.
    assert!(contrast(LIGHT.line, LIGHT.panel) >= 1.2);
}

#[test]
fn dark_palette_meets_the_same_bar() {
    for bg in [DARK.ink, DARK.panel, DARK.raised] {
        for fg in [DARK.text, DARK.muted, DARK.accent, DARK.danger, DARK.warn, DARK.success] {
            assert!(contrast(fg, bg) >= 4.5, "DARK fg {fg:?} on {bg:?}");
        }
    }
}
```

(MEASURE the dark ratios first — if any shipped DARK pairing fails 4.5, do NOT change DARK
(its identity is frozen); instead lower ONLY the dark test's threshold to the measured floor
with a comment naming the actual worst ratio and that the AA bar is a LIGHT-palette gate.
Report the measured dark ratios either way.)

- [ ] **Step 2: Run** — FAIL (LIGHT undefined). **Step 3:** add LIGHT; tune any failing channel minimally (keep hue intent; adjust lightness). Record final values + ratios in the report.
- [ ] **Step 4:** full suite + both clippy. **Step 5: Commit** `feat(gui): the LIGHT palette, WCAG-AA machine-checked`

### Task 3: ThemePref, resolution, persistence, App wiring

**Files:**
- Modify: `springmaker/src/settings.rs` (AppSettings.theme_pref)
- Modify: `springmaker/src/app.rs` (ThemePref, fields, messages, pal()/theme())
- Test: both files' test modules

**Interfaces:**
- Produces (settings.rs):

```rust
/// Theme preference: follow the OS, or force a palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}
```
  and `AppSettings { curvature_correction, pub theme_pref: ThemePref }` (`#[serde(default)]` on the struct already covers old files).
- Produces (app.rs): `App.theme_pref: ThemePref` (from settings at construction — `App::from_store` gains a `theme_pref` parameter? NO: keep the constructor signature stable for tests by defaulting to `ThemePref::default()` in `from_store` and having `initial_app` in main.rs assign `app.theme_pref = settings.theme_pref;` — mirrors how `settings_path` is assigned post-construction); `App.system_mode: iced::theme::Mode` (init `Mode::default()` = None); `Message::ThemePref(ThemePref)`, `Message::SystemTheme(iced::theme::Mode)`;

```rust
/// The palette for a pref × OS-mode pair. `Mode::None` (OS reported nothing)
/// resolves to DARK — the shipped identity.
pub(crate) fn resolved_palette(pref: ThemePref, system: iced::theme::Mode) -> &'static Palette {
    match pref {
        ThemePref::Dark => &DARK,
        ThemePref::Light => &LIGHT,
        ThemePref::System => match system {
            iced::theme::Mode::Light => &LIGHT,
            iced::theme::Mode::Dark | iced::theme::Mode::None => &DARK,
        },
    }
}
```
  `App::pal()` becomes `resolved_palette(self.theme_pref, self.system_mode)`. `App::theme()` builds `Theme::custom` from `self.pal()` tokens with the name `"OpenSpringmaker Light"` / `"OpenSpringmaker Dark"` picked by `std::ptr::eq(self.pal(), &LIGHT)` — or cleaner, match on the same resolution (bake-in GONE; the identity test may keep using DARK directly).
- Message arms (both return `false` — no solver recompute; iced re-renders after every update regardless):

```rust
Message::ThemePref(p) => {
    if set_if_changed(&mut self.theme_pref, p) {
        self.persist_settings(); // extract the existing SetCorrection save block into
                                 // fn persist_settings(&mut self) so BOTH prefs share the
                                 // save + settings_error handling verbatim
    }
    false
}
Message::SystemTheme(mode) => {
    let _ = set_if_changed(&mut self.system_mode, mode);
    false
}
```
  (`SetCorrection`'s arm switches to `persist_settings` too — one save path. Its save payload now includes BOTH fields: build `AppSettings { curvature_correction: self.correction, theme_pref: self.theme_pref }` inside `persist_settings`.)

- [ ] **Step 1: Write the failing tests** — settings.rs: round-trip with `theme_pref: ThemePref::Light`; old-file back-compat (`load_from` on a TOML containing ONLY `curvature_correction = "wahl"` → `theme_pref == System`, no warning). app.rs: resolution table:

```rust
#[test]
fn resolved_palette_covers_the_pref_by_mode_matrix() {
    use iced::theme::Mode::*;
    let cases: [(ThemePref, iced::theme::Mode, &Palette); 9] = [
        (ThemePref::Dark, None, &DARK), (ThemePref::Dark, Light, &DARK), (ThemePref::Dark, Dark, &DARK),
        (ThemePref::Light, None, &LIGHT), (ThemePref::Light, Light, &LIGHT), (ThemePref::Light, Dark, &LIGHT),
        (ThemePref::System, None, &DARK), (ThemePref::System, Light, &LIGHT), (ThemePref::System, Dark, &DARK),
    ];
    for (pref, mode, want) in cases {
        assert!(std::ptr::eq(resolved_palette(pref, mode), want), "{pref:?} × {mode:?}");
    }
}

#[test]
fn theme_pref_message_persists_and_switches_the_palette() {
    let mut app = test_app_with_writable_settings(); // temp-dir settings_path; follow the
                                                     // settings retry test's temp idiom
    app.update(Message::ThemePref(ThemePref::Light));
    assert!(std::ptr::eq(app.pal(), &LIGHT));
    let (saved, _) = crate::settings::load_from(app.settings_path.as_ref().unwrap());
    assert_eq!(saved.theme_pref, ThemePref::Light);
    assert_eq!(saved.curvature_correction, app.correction, "one save path carries BOTH prefs");
}

#[test]
fn theme_messages_do_not_recompute_or_touch_error_channels() {
    // action_error sentinel + solved outcome; ThemePref/SystemTheme flips leave both intact
    // (mirror probe_visual_message_preserves_error_channels's shape).
}
```

  (Write the third test fully — the sketch names the contract; follow the named test's real shape.)
- [ ] **Step 2: RED. Step 3: implement per Interfaces (incl. `persist_settings` extraction). Step 4: full suite + both clippy. Step 5: Commit** `feat(gui): theme preference — resolution, persistence, message wiring`

### Task 4: Settings picker + ViewModel clickable flag

**Files:**
- Modify: `springmaker/src/settings_view_model.rs`, `springmaker/src/settings_view.rs`
- Test: both + `springmaker/src/ui_tests.rs`

**Interfaces:**
- Produces (settings_view_model.rs): `CorrectionOption` gains `pub clickable: bool`; new

```rust
pub struct ThemeOption {
    pub value: crate::settings::ThemePref,
    pub label: String,
    pub selected: bool,
    pub clickable: bool,
}
```
  `SettingsViewModel` gains `pub theme_options: Vec<ThemeOption>` (labels exactly "System"/"Light"/"Dark"). Clickability RULE (one place, the VM): `clickable = !selected || save_feedback_pending` where `save_feedback_pending = app.settings_error.is_some()` — the retry affordance decision moves OUT of the view (carried flag #3; the view's "no logic" module doc becomes true).
- settings_view.rs: the options loop reads `option.clickable` (`if opt.clickable { btn = btn.on_press(...) }`); the theme group renders under `section_heading(pal, "Theme")` + divider below the correction panel, same prose-button pattern (labels are short; the shared `segmented_style` still styles them). Both groups run through ONE local render helper (`fn option_button<'a>(pal, label, clickable, msg) -> Element<'a, Message>`).

- [ ] **Step 1: Failing VM tests** — theme_options marks the active pref selected + exactly 3 options + labels exact; clickable rule table (selected+no-error → false; selected+error → true; unselected → true) for BOTH option kinds.
- [ ] **Step 2: Failing ui_tests** —

```rust
#[test]
fn theme_picker_switches_to_light_by_clicking_the_label() {
    let mut app = test_app();
    app.update(Message::NavigateTo(Screen::Settings));
    click(&mut app, "Light");
    assert!(std::ptr::eq(app.pal(), &crate::app::LIGHT));
    assert!(shows(&app, "Theme"));
}
```
  plus a no-op reclick sentinel test on the selected theme option (mirror the correction one).
- [ ] **Step 3: RED (both layers). Step 4: implement. Step 5: full suite + both clippy. Step 6: Commit** `feat(gui): theme picker in Settings — ViewModel-owned clickability`

### Task 5: OS-theme integration + differential pins + full gate

**Files:**
- Modify: `springmaker/src/main.rs` (subscription + startup seed)
- Modify: `springmaker/src/app.rs` (subscription fn)
- Test: `springmaker/src/ui_tests.rs`, `springmaker/src/plot/render.rs`

**Interfaces:**
- Produces (app.rs): `pub fn subscription(&self) -> iced::Subscription<Message> { iced::system::theme_changes().map(Message::SystemTheme) }`
- main.rs: `.subscription(App::subscription)` on the builder; startup seed = boot returns the task: change `iced::application(initial_app, …)` to a boot closure returning `(App, iced::Task<app::Message>)`:

```rust
fn boot() -> (App, iced::Task<app::Message>) {
    (initial_app(), iced::system::theme().map(app::Message::SystemTheme))
}
```
  VERIFY in the vendored source that `iced::application`'s boot accepts `(State, Task<Message>)` (iced-0.14.0/src/application.rs — the `Boot` trait impls near the top). If it does not, FALLBACK (documented in the report): keep `initial_app` and chain the seed off the subscription's first event only, accepting no-seed-until-first-change; prefer the boot-task if at all possible.
- Differential pins:

```rust
// ui_tests.rs — the mode switch changes rendered output (settings screen, one isolated pair).
#[test]
fn theme_switch_changes_the_rendered_settings_screen() {
    let mut app = test_app();
    app.update(Message::NavigateTo(Screen::Settings));
    let dark = snapshot_hash(&app, "theme-dark");
    app.update(Message::ThemePref(crate::settings::ThemePref::Light));
    let light = snapshot_hash(&app, "theme-light");
    assert_ne!(dark, light, "switching the palette must change rendered pixels");
}

// plot/render.rs tests — bitmaps follow the ACTIVE palette end-to-end.
#[test]
fn render_chart_backgrounds_differ_between_dark_and_light() {
    let (d, _) = render_chart(&DARK, &simple_data(false)).unwrap();
    let (l, _) = render_chart(&LIGHT, &simple_data(false)).unwrap();
    assert_ne!(d[0..3], l[0..3]);
}
```
  plus a calculator-screen both-themes smoke (solve compression, flip pref to Light, assert `shows("Spring rate")` still true and no placeholder text — the light render works end-to-end, not just settings).
- [ ] **Step 1: RED (subscription fn missing / pins fail). Step 2: implement. Step 3:** full gate — `cargo fmt --all --check`; `cargo test --workspace`; BOTH clippy; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`; `typos`. Record counts.
- [ ] **Step 4: Commit** `feat(gui): live OS-theme tracking + theme differential pins`

---

## Self-review notes (applied)

- Spec §PR 2 coverage: LIGHT+AA (T2), resolution/pref/System default (T3), Settings segmented-pattern picker (T4 — prose-button variant, consistent with the correction options; spec's "segmented control" wording covered by the shared style, same PR-1 adjudication), persistence via the settings store (T3), subscription+seed (T5), renderer awareness (T5 pins; no cache to invalidate). Carried flags: derived colors (T1), theme() bake-in (T3), VM clickable (T4), snapshot pins (T5).
- Five family views untouched: no task lists them; the final gate diff-checks it (`git diff main -- springmaker/src/{compression,extension,torsion,conical,assembly}/ → empty`). Add that check to Task 5 Step 3.
- Type consistency: `ThemePref` lives in settings.rs (persistence home) and is re-exported/used via `crate::settings::ThemePref` everywhere; `resolved_palette` in app.rs; labels exact-match between VM tests and ui_tests clicks.
- Known judgment points for implementers (record in reports): LIGHT value tuning against the contrast test; the boot-task verification (T5); whether `Theme::custom` needs per-build caching (it's built per theme() call — iced calls it per frame; if profiling shows churn, memoize on (pref, mode) — do NOT pre-optimize).
