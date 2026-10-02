//! Saved-recipe library: category filter, fuzzy search, select-to-load, delete.

use egui::{Color32, Painter, Rect, Stroke};

use tfc_forge::SavedRecipe;

use crate::app::ForgeApp;
use crate::theme;
use crate::ui;

/// The library search box id, shared with the shortcut handler.
pub fn search_id() -> egui::Id {
    egui::Id::new("tfc-library-search")
}

/// Edge length of the compact square icon-only delete control.
pub const DELETE_BUTTON_SIZE: f32 = 24.0;

/// The single action a library-row frame resolves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowClick {
    None,
    Load,
    Delete,
}

/// Resolve the row and delete-icon click flags into at most one action.
///
/// The delete icon is the topmost hit target, so when both flags somehow
/// register in the same frame the icon wins: a click on the icon must never
/// also load the recipe behind it.
pub fn resolve_row_click(row_clicked: bool, delete_clicked: bool) -> RowClick {
    if delete_clicked {
        RowClick::Delete
    } else if row_clicked {
        RowClick::Load
    } else {
        RowClick::None
    }
}

/// Paint a compact trash-can delete glyph from line primitives only.
///
/// Drawing the icon instead of using a text label keeps the source ASCII-only
/// and needs no font glyph, image asset, or new dependency.
pub fn paint_trash_icon(painter: &Painter, rect: Rect, color: Color32) {
    let w = rect.width();
    let h = rect.height();
    let x = |fraction: f32| rect.left() + w * fraction;
    let y = |fraction: f32| rect.top() + h * fraction;
    let stroke = Stroke::new(1.5, color);

    // Lid handle, then the lid itself.
    painter.line_segment(
        [egui::pos2(x(0.38), y(0.20)), egui::pos2(x(0.62), y(0.20))],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(x(0.16), y(0.30)), egui::pos2(x(0.84), y(0.30))],
        stroke,
    );
    // Tapered body: two sides and the bottom.
    painter.line_segment(
        [egui::pos2(x(0.26), y(0.34)), egui::pos2(x(0.31), y(0.82))],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(x(0.74), y(0.34)), egui::pos2(x(0.69), y(0.82))],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(x(0.31), y(0.82)), egui::pos2(x(0.69), y(0.82))],
        stroke,
    );
    // Two interior ribs.
    painter.line_segment(
        [egui::pos2(x(0.43), y(0.42)), egui::pos2(x(0.445), y(0.74))],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(x(0.57), y(0.42)), egui::pos2(x(0.555), y(0.74))],
        stroke,
    );
}

/// Adaptive list height: a large fraction of the viewport so the library
/// consumes real screen space instead of being capped at a fixed 360px.
fn adaptive_list_height(ui: &egui::Ui) -> f32 {
    let viewport = ui.ctx().viewport_rect().height();
    (viewport * 0.60).clamp(320.0, 1400.0)
}

/// Render the library section.
pub fn show(app: &mut ForgeApp, ui: &mut egui::Ui) {
    theme::section_title(ui, "Library");

    // Category filter. Scoping the spacing makes only this control a little
    // taller, leaving the action/rule buttons and the compact category
    // suggestion trigger at their existing height.
    let categories = ui::category_options(&app.recipes);
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = theme::INPUT_HEIGHT;
        egui::ComboBox::from_id_salt("tfc-category-filter")
            .selected_text(if app.category_filter.is_empty() {
                "All categories".to_string()
            } else {
                app.category_filter.clone()
            })
            .width(180.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(app.category_filter.is_empty(), "All categories")
                    .clicked()
                {
                    app.category_filter.clear();
                }
                for category in &categories {
                    let selected = app.category_filter == *category;
                    if ui.selectable_label(selected, category).clicked() {
                        app.category_filter = category.clone();
                    }
                }
            });
    });

    ui.add_space(4.0);
    let response = ui.add(
        egui::TextEdit::singleline(&mut app.search)
            .id(search_id())
            .hint_text("Search (Ctrl/Cmd+F)")
            .min_size(egui::vec2(0.0, theme::INPUT_HEIGHT))
            .desired_width(f32::INFINITY),
    );
    if app.focus_search {
        response.request_focus();
        app.focus_search = false;
    }
    if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        app.search.clear();
    }

    if let Some(error) = &app.library_error {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(error).size(12.0).color(theme::RED));
    }

    ui.add_space(8.0);
    let filtered = ui::filter_by_category(&app.recipes, &app.category_filter);
    let visible = ui::fuzzy_filter(&filtered, &app.search);

    if visible.is_empty() {
        ui.label(
            egui::RichText::new("No matches")
                .size(12.5)
                .color(theme::MUTED),
        );
        return;
    }

    let mut load: Option<SavedRecipe> = None;
    let mut delete: Option<String> = None;
    // The library sits inside a zero-height column allocation, where the
    // available height is not the viewport. Set both bounds to the adaptive
    // height so the list keeps its real size instead of collapsing.
    let list_height = adaptive_list_height(ui);
    egui::ScrollArea::vertical()
        .min_scrolled_height(list_height)
        .max_height(list_height)
        .auto_shrink([false, false])
        .id_salt("tfc-library-list")
        .show(ui, |ui| {
            for recipe in &visible {
                let selected = app.selected_id.as_deref() == Some(recipe.id.as_str());
                let armed = app.is_delete_armed(&recipe.id);
                let label = ui::saved_label(recipe);
                let meta = format!(
                    "{} to {} | {} steps | {}",
                    recipe.start,
                    recipe.target,
                    recipe.actions.len(),
                    if recipe.compact.is_empty() {
                        "none"
                    } else {
                        &recipe.compact
                    }
                );

                // Flat row: no card frame or per-row border, only a selection /
                // hover wash and an optional bottom hairline. The taller padding
                // and larger label give the row comfortable breathing room.
                let mut prepared = egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(8, 10))
                    .begin(ui);

                // Top line: the recipe label, then one compact square icon-only
                // delete control at the trailing edge. The label is given a
                // bounded width (the visible row width minus the icon and the
                // gap) and truncated, so the control is never pushed off the
                // visible row.
                let delete_rect = prepared
                    .content_ui
                    .horizontal(|ui| {
                        // Inside nested scroll/column layouts `available_width`
                        // can exceed the visible viewport, which would push the
                        // control past the clip rect. Cap it by the space
                        // actually left before the clip rect's right edge so the
                        // label and the control always land on screen.
                        let clip_right = ui.clip_rect().right();
                        let visible_width = (clip_right - ui.cursor().left())
                            .min(ui.available_width())
                            .max(60.0);
                        let spacing = ui.spacing().item_spacing.x;
                        let label_width = (visible_width - DELETE_BUTTON_SIZE - spacing).max(60.0);
                        ui.scope(|ui| {
                            ui.set_width(label_width);
                            // Display only: the whole row handles the click, so
                            // the label no longer needs its own Load action or
                            // tooltip.
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&label)
                                        .strong()
                                        .size(14.0)
                                        .color(theme::TEXT),
                                )
                                .truncate(),
                            );
                        });
                        // Reserve the square; the real click target is
                        // registered below, after the row, so it stays on top.
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(DELETE_BUTTON_SIZE, DELETE_BUTTON_SIZE),
                            egui::Sense::hover(),
                        );
                        rect
                    })
                    .inner;

                // Compact metadata below the top line, with a little space so
                // the two lines read as a unit.
                prepared.content_ui.add_space(3.0);
                prepared
                    .content_ui
                    .label(egui::RichText::new(&meta).size(12.0).color(theme::MUTED));

                // Allocate the row, then register the row click followed by the
                // icon click. Registration order is what makes the icon the
                // topmost click target wherever the two hit areas overlap, so a
                // delete click can never also load the row.
                let response = prepared.allocate_space(ui);
                let row_response = ui.interact(
                    response.rect,
                    egui::Id::new(("tfc-library-row", recipe.id.as_str())),
                    egui::Sense::click(),
                );
                let delete_response = ui
                    .interact(
                        delete_rect,
                        egui::Id::new(("tfc-library-delete", recipe.id.as_str())),
                        egui::Sense::click(),
                    )
                    .on_hover_text(if armed {
                        "Click again to confirm delete"
                    } else {
                        "Delete this recipe"
                    });

                // Icon-only control: a filled red confirmation state when armed.
                {
                    let painter = ui.painter();
                    if armed {
                        painter.rect_filled(delete_rect, 3.0, theme::RED);
                        paint_trash_icon(painter, delete_rect, theme::TEXT);
                    } else {
                        let hovered = delete_response.hovered();
                        if hovered {
                            painter.rect_filled(delete_rect, 3.0, theme::SURFACE_3);
                        }
                        let color = if hovered { theme::RED } else { theme::MUTED };
                        paint_trash_icon(painter, delete_rect, color);
                    }
                }

                // Choose the fill from the row's own hover, then paint it into
                // the placeholder that Frame inserted behind the content. The
                // hover-sense response still covers the full row (including the
                // icon column), preserving the full-width wash.
                prepared.frame.fill = if selected {
                    theme::SELECTION
                } else if response.contains_pointer() || row_response.hovered() {
                    theme::SURFACE_3
                } else {
                    egui::Color32::TRANSPARENT
                };
                prepared.paint(ui);
                ui.painter().line_segment(
                    [response.rect.left_bottom(), response.rect.right_bottom()],
                    Stroke::new(1.0, theme::LINE_SOFT),
                );

                match resolve_row_click(row_response.clicked(), delete_response.clicked()) {
                    RowClick::Load => load = Some(recipe.clone()),
                    RowClick::Delete => delete = Some(recipe.id.clone()),
                    RowClick::None => {}
                }
                ui.add_space(6.0);
            }
        });

    if let Some(recipe) = load {
        app.load_recipe(&recipe);
    }
    if let Some(id) = delete {
        // The first click only arms; the second confirms the delete.
        if app.click_delete(&id) {
            app.delete_recipe(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_row_click_never_loads_on_delete() {
        assert_eq!(resolve_row_click(false, false), RowClick::None);
        assert_eq!(resolve_row_click(true, false), RowClick::Load);
        assert_eq!(resolve_row_click(false, true), RowClick::Delete);
        // The icon is the delete hit; an overlapping row click must not load.
        assert_eq!(resolve_row_click(true, true), RowClick::Delete);
    }

    #[test]
    fn trash_icon_is_drawn_from_several_line_primitives() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(DELETE_BUTTON_SIZE, DELETE_BUTTON_SIZE),
                egui::Sense::hover(),
            );
            paint_trash_icon(ui.painter(), rect, theme::TEXT);
        });
        let mut lines = 0usize;
        for shape in &output.shapes {
            count_line_segments(&shape.shape, &mut lines);
        }
        output.drop_without_applying_deltas();
        assert!(
            lines >= 5,
            "trash icon should paint several line segments, got {lines}"
        );
    }

    fn count_line_segments(shape: &egui::epaint::Shape, out: &mut usize) {
        match shape {
            egui::epaint::Shape::LineSegment { .. } => *out += 1,
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    count_line_segments(shape, out);
                }
            }
            _ => {}
        }
    }
}
