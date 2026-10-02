//! Central neutral palette and egui visual style for the TFC Forge Planner.
//!
//! Every widget in [`crate::ui`] reads colors from here so the look can be
//! adjusted in one place. Only standard egui painting is used; no webview or
//! HTML is involved.

use egui::{Color32, CornerRadius, Margin, Stroke, Visuals};

/// Top of the central-panel background gradient (near black).
pub const BG_TOP: Color32 = Color32::from_rgb(0x10, 0x10, 0x10);
/// Bottom of the central-panel background gradient (dark charcoal).
pub const BG_BOTTOM: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x1e);
/// Deepest neutral surface.
pub const BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x18);
/// Base surface.
pub const PANEL: Color32 = Color32::from_rgb(0x18, 0x18, 0x18);
/// Raised surface used by tiles and controls.
pub const PANEL_2: Color32 = Color32::from_rgb(0x20, 0x20, 0x20);
/// Highest neutral surface (hover).
pub const SURFACE_3: Color32 = Color32::from_rgb(0x25, 0x25, 0x26);
/// Hairline separator.
pub const LINE: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
/// Softer interior line.
pub const LINE_SOFT: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2a);
/// Body text.
pub const TEXT: Color32 = Color32::from_rgb(0xd4, 0xd4, 0xd4);
/// Muted / secondary text.
pub const MUTED: Color32 = Color32::from_rgb(0x8a, 0x8a, 0x8a);
/// Restrained editor-blue accent for focus and selection.
pub const ACCENT: Color32 = Color32::from_rgb(0x3d, 0x7c, 0xd6);
/// Dim accent for hover/active fills.
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x2a, 0x4f, 0x86);
/// Flat selection background for selected rows.
pub const SELECTION: Color32 = Color32::from_rgb(0x26, 0x3a, 0x57);
/// Current-work green (semantic only).
pub const GREEN: Color32 = Color32::from_rgb(0x4e, 0xc2, 0x6a);
/// Target / error red (semantic only).
pub const RED: Color32 = Color32::from_rgb(0xe0, 0x52, 0x52);
/// Input field background.
pub const FIELD_BG: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);
/// Darkest inset background for rails and tracks.
pub const INSET: Color32 = Color32::from_rgb(0x0d, 0x0d, 0x0d);

/// Outer height of the editable/selectable input controls: the Current/Target
/// drag values, the Save category/name text fields, and the Library category
/// filter and search field. A few pixels taller than the 22px button height so
/// text entry reads as an input, without resizing action or rule buttons.
pub const INPUT_HEIGHT: f32 = 25.0;

/// Apply the neutral editor theme to the whole egui context.
pub fn install(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    visuals.dark_mode = true;
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL_2;
    visuals.extreme_bg_color = FIELD_BG;
    visuals.faint_bg_color = PANEL;
    visuals.text_edit_bg_color = Some(FIELD_BG);
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = SELECTION;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = ACCENT;
    visuals.error_fg_color = RED;
    visuals.window_corner_radius = CornerRadius::ZERO;
    visuals.window_stroke = Stroke::new(1.0, LINE);
    visuals.window_shadow = egui::epaint::Shadow::NONE;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = PANEL;
    widgets.noninteractive.weak_bg_fill = PANEL;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
    widgets.noninteractive.corner_radius = CornerRadius::ZERO;

    widgets.inactive.bg_fill = PANEL_2;
    widgets.inactive.weak_bg_fill = PANEL_2;
    widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
    widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.inactive.corner_radius = CornerRadius::ZERO;

    widgets.hovered.bg_fill = SURFACE_3;
    widgets.hovered.weak_bg_fill = SURFACE_3;
    widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.hovered.corner_radius = CornerRadius::ZERO;

    widgets.active.bg_fill = ACCENT_DIM;
    widgets.active.weak_bg_fill = ACCENT_DIM;
    widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.active.corner_radius = CornerRadius::ZERO;

    widgets.open.bg_fill = PANEL_2;
    widgets.open.weak_bg_fill = PANEL_2;
    widgets.open.bg_stroke = Stroke::new(1.0, ACCENT);
    widgets.open.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.open.corner_radius = CornerRadius::ZERO;

    ctx.all_styles_mut(|style| {
        style.visuals = visuals.clone();
        style.spacing.item_spacing = egui::vec2(6.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.window_margin = Margin::same(12);
        style.spacing.interact_size.y = 22.0;
    });
}

/// Paint the vertical black-to-charcoal gradient across `rect`.
pub fn paint_gradient(painter: &egui::Painter, rect: egui::Rect) {
    painter.add(egui::Shape::gradient_rect(
        rect,
        egui::Direction::TopDown,
        [BG_TOP, BG_BOTTOM],
    ));
}

/// A one-pixel horizontal separator across the available width.
pub fn hairline(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, LINE);
}

/// Draw a concise section title followed by a hairline separator.
pub fn section_title(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).size(13.0).strong().color(TEXT));
    ui.add_space(2.0);
    hairline(ui);
    ui.add_space(8.0);
}
