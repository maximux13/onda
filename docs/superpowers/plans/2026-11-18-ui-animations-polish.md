# Onda Smooth Tray Animations & UI Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add smooth 120Hz GPUI entrance/view-switch animations and refined glassmorphic visual polish to the Onda menu bar popover.

**Architecture:** Leverage GPUI `0.2.2`'s native `Animation`, `AnimationExt::with_animation`, `gpui::ease_out_quint()`, and `Styled::opacity` on `TrayPopoverView` and its modular submodules (`src/ui/popover/`), backed by pure interpolation helpers with unit tests.

**Tech Stack:** Rust (Edition 2024), GPUI `0.2.2` (`Animation`, `AnimationExt`, `ease_out_quint`, `ease_in_out`), `ThemePalette` (`src/ui/mod.rs`).

## Global Constraints

- Rust Edition 2024 (`Cargo.toml`); maintain `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, and `cargo test` green after every task.
- Respect `[lints.rust]` and `[lints.clippy]` in `Cargo.toml`; never silence warnings with `#[allow(...)]` without justification.
- Keep animations fast and snappy (`140ms–200ms`) using `gpui::ease_out_quint()` so the menu bar popover feels native to macOS.
- Name tests descriptively (`<unit>_should_<behavior>_when_<condition>`).

---

### Task 1: Smooth Popover Entrance Animation (`src/ui/popover/mod.rs`)

**Files:**
- Modify: `src/ui/popover/mod.rs`

**Interfaces:**
- Produces in `src/ui/popover/mod.rs`:
  - `fn popover_enter_metrics(delta: f32) -> (f32, f32)` returning `(opacity, top_offset_px)`
  - `Render for TrayPopoverView` wrapping the root popover container with `AnimationExt::with_animation`

- [ ] **Step 1: Write unit tests for `popover_enter_metrics` in `src/ui/popover/mod.rs`**

```rust
fn popover_enter_metrics(delta: f32) -> (f32, f32) {
    let t = delta.clamp(0.0, 1.0);
    let opacity = t;
    let top_offset_px = (1.0 - t) * -8.0;
    (opacity, top_offset_px)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popover_enter_metrics_should_start_transparent_and_offset_when_delta_zero() {
        let (opacity, offset) = popover_enter_metrics(0.0);
        assert_eq!(opacity, 0.0);
        assert_eq!(offset, -8.0);
    }

    #[test]
    fn popover_enter_metrics_should_settle_opaque_and_aligned_when_delta_one() {
        let (opacity, offset) = popover_enter_metrics(1.0);
        assert_eq!(opacity, 1.0);
        assert_eq!(offset, 0.0);
    }

    #[test]
    fn popover_enter_metrics_should_clamp_out_of_range_deltas() {
        assert_eq!(popover_enter_metrics(-0.5), (0.0, -8.0));
        assert_eq!(popover_enter_metrics(1.5), (1.0, 0.0));
    }
}
```

- [ ] **Step 2: Run tests to verify compilation/behavior**

Run: `cargo test ui::popover::tests`
Expected: PASS

- [ ] **Step 3: Apply `Animation::new(Duration::from_millis(180)).with_easing(gpui::ease_out_quint())` to `TrayPopoverView::render`**

In `src/ui/popover/mod.rs`:
Import `Animation`, `AnimationExt`, `ease_out_quint` from `gpui`, and wrap the root `div()` in `Render::render` with:
```rust
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .p(px(16.0))
            .rounded(px(20.0))
            .bg(palette.panel_bg)
            .border_1()
            .border_color(palette.panel_border)
            .shadow_lg()
            .text_color(palette.text_primary)
            .child(content)
            .with_animation(
                "popover-enter",
                Animation::new(Duration::from_millis(180)).with_easing(ease_out_quint()),
                |el, delta| {
                    let (opacity, offset_px) = popover_enter_metrics(delta);
                    el.opacity(opacity).mt(px(offset_px))
                },
            )
```

- [ ] **Step 4: Run full verification**

Run: `cargo fmt && cargo clippy --all-targets --all-features --locked -- -D warnings && cargo test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/ui/popover/mod.rs
git commit -m "feat(ui): add smooth ease-out entrance animation to tray popover"
```

---

### Task 2: Smooth View & Tab Transitions (`src/ui/popover/mod.rs`, `src/ui/popover/settings_view.rs`)

**Files:**
- Modify: `src/ui/popover/mod.rs`
- Modify: `src/ui/popover/settings_view.rs`

**Interfaces:**
- Modifies `TrayPopoverView` in `src/ui/popover/mod.rs` to track `view_transition_id: u64` incremented whenever `show_settings` or `settings_tab` changes, so GPUI replays a crisp `150ms` `ease_out_quint()` transition on view/tab switches.

- [ ] **Step 1: Add `view_transition_id: u64` and `pub(super) fn bump_view_transition(&mut self)` to `TrayPopoverView`**

In `src/ui/popover/mod.rs`:
- Initialize `view_transition_id: 0` in `TrayPopoverView::new`.
- Add `pub(super) fn bump_view_transition(&mut self) { self.view_transition_id = self.view_transition_id.wrapping_add(1); }`.
- Wrap `content` in `Render::render` with `.with_animation(("view-switch", self.view_transition_id), Animation::new(Duration::from_millis(150)).with_easing(ease_out_quint()), |el, delta| el.opacity(delta))`.

- [ ] **Step 2: Call `self.bump_view_transition()` on Settings toggle and Settings tab selection**

Update the `show_settings` toggle listeners in `src/ui/popover/mixer_view.rs` / `src/ui/popover/settings_view.rs` and `settings_tab` click handlers in `src/ui/popover/settings_view.rs` to call `self.bump_view_transition()` before `cx.notify()`.

- [ ] **Step 3: Run full verification**

Run: `cargo fmt && cargo clippy --all-targets --all-features --locked -- -D warnings && cargo test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/ui/popover/mod.rs src/ui/popover/mixer_view.rs src/ui/popover/settings_view.rs
git commit -m "feat(ui): animate transitions between mixer, settings, and tabs"
```

---

### Task 3: Visual Polish for Cards, Sliders, Toggles & Routing (`src/ui/mod.rs`, `src/ui/slider.rs`, `src/ui/popover/`)

**Files:**
- Modify: `src/ui/mod.rs`
- Modify: `src/ui/slider.rs`
- Modify: `src/ui/popover/app_row.rs`
- Modify: `src/ui/popover/device_row.rs`
- Modify: `src/ui/popover/mixer_view.rs`

**Interfaces:**
- Refines `ThemePalette` contrast/surfaces (`src/ui/mod.rs`) and adds interactive hover/thumb polish in `VolumeSlider`, `EqVerticalSlider`, `app_row.rs`, and `device_row.rs`.

- [ ] **Step 1: Refine `ThemePalette` surface contrast and border tokens (`src/ui/mod.rs`)**

Tune `panel_border`, `card_bg`, `card_border`, `row_hover_bg`, and `pill_bg` in `ThemePalette::for_theme` for crisper glassmorphic depth and contrast in both Dark and Light modes.

- [ ] **Step 2: Enhance `VolumeSlider` & `EqVerticalSlider` visual polish (`src/ui/slider.rs`)**

- Add subtle thumb border/shadow and hover highlight on the track/thumb.
- Keep `normalized_axis_ratio` and `bind_drag_handlers` clean and add unit tests for `VolumeSlider::value_from_x` and `EqVerticalSlider::gain_from_y` boundary clamping.

- [ ] **Step 3: Polish `app_row.rs`, `device_row.rs`, and `mixer_view.rs` spacing & language consistency**

- Replace the stray Spanish `"Activo"` badge text in `src/ui/popover/app_row.rs` with `"Active"` so the UI is consistently English.
- Refine hover states, rounded corners, and VU meter bar spacing in `app_row.rs` and `device_row.rs`.

- [ ] **Step 4: Run full verification & bundle `.app` to smoke-test**

Run: `cargo fmt && cargo clippy --all-targets --all-features --locked -- -D warnings && cargo test && ./scripts/bundle_app.sh release`
Expected: PASS and `Built target/release/Onda.app`

- [ ] **Step 5: Commit**

```bash
git add src/ui/
git commit -m "feat(ui): polish glassmorphic palette, slider thumbs, and row interactions"
```
