//! Solution display and the save section.

use egui::{Color32, Stroke, vec2};

use tfc_forge::ForgeSolution;

use crate::app::{Feedback, ForgeApp};
use crate::theme;
use crate::ui;

/// Render the solution section: sequence, summary line, path, step table.
pub fn show(app: &mut ForgeApp, ui: &mut egui::Ui) {
    theme::section_title(ui, "Solution");

    if !app.rule_status().is_valid() {
        ui.label(
            egui::RichText::new(
                app.rule_status()
                    .message()
                    .unwrap_or("Add at least one rule to compute a sequence."),
            )
            .color(theme::MUTED),
        );
        return;
    }

    match app.solution.clone() {
        None => no_solution(app, ui),
        Some(solution) => rendered_solution(app, ui, &solution),
    }
}

/// Shown when the range/rules have no sequence.
fn no_solution(_app: &ForgeApp, ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("No solution")
            .size(13.0)
            .strong()
            .color(theme::RED),
    );
}

/// The full solution body when a sequence exists.
fn rendered_solution(_app: &mut ForgeApp, ui: &mut egui::Ui, solution: &ForgeSolution) {
    // Compact sequence plus copy button, inline on the section surface.
    ui.horizontal_wrapped(|ui| {
        let compact = if solution.compact.is_empty() {
            "(no actions needed)".to_string()
        } else {
            solution.compact.clone()
        };
        ui.label(
            egui::RichText::new(compact)
                .size(15.0)
                .strong()
                .monospace()
                .color(theme::TEXT),
        );
        if ui
            .button("Copy")
            .on_hover_text("Copy the compact sequence to the clipboard")
            .clicked()
        {
            ui.ctx().copy_text(solution.compact.clone());
        }
    });

    // One compact monospaced summary line, no background.
    let min = solution.path.iter().copied().min().unwrap_or(0);
    let max = solution.path.iter().copied().max().unwrap_or(0);
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(format!(
            "{} steps  |  min {}  |  max {}",
            solution.actions.len(),
            min,
            max
        ))
        .size(11.5)
        .monospace()
        .color(theme::MUTED),
    );

    // Numeric path strip.
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        for (index, value) in solution.path.iter().enumerate() {
            let is_target = index == solution.path.len() - 1;
            let color = if is_target { theme::RED } else { theme::TEXT };
            ui.label(
                egui::RichText::new(value.to_string())
                    .size(12.0)
                    .monospace()
                    .color(color),
            );
        }
    });

    // Per-step table.
    ui.add_space(8.0);
    step_table(ui, solution);
}

/// The signed delta text for an action (`+7`, `-3`).
fn signed_delta(delta: i32) -> String {
    if delta >= 0 {
        format!("+{delta}")
    } else {
        delta.to_string()
    }
}

/// A strong, muted table header label.
fn header_text(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(11.0)
        .strong()
        .color(theme::MUTED)
}

/// Right-align a single label within a cell.
fn right_aligned(ui: &mut egui::Ui, text: egui::RichText) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(text);
    });
}

/// The per-step action table: a distinct header, comfortable rows, subtle
/// striping, and its own vertical scroll so long sequences stay reachable.
fn step_table(ui: &mut egui::Ui, solution: &ForgeSolution) {
    use egui_extras::{Column, TableBuilder};

    TableBuilder::new(ui)
        .id_salt("tfc-step-table")
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(30.0))
        .column(Column::remainder().at_least(90.0))
        .column(Column::exact(64.0))
        .column(Column::exact(64.0))
        .min_scrolled_height(180.0)
        .max_scroll_height(260.0)
        .auto_shrink([false, true])
        .header(26.0, |mut header| {
            header.col(|ui| {
                right_aligned(ui, header_text("#"));
            });
            header.col(|ui| {
                ui.label(header_text("Action"));
            });
            header.col(|ui| {
                right_aligned(ui, header_text("Delta"));
            });
            header.col(|ui| {
                right_aligned(ui, header_text("Work"));
            });
        })
        .body(|body| {
            body.rows(24.0, solution.actions.len(), |mut row| {
                let index = row.index();
                let action = solution.actions[index];
                let def = tfc_forge::action_def(action);
                let work = solution.path.get(index + 1).copied().unwrap_or_default();

                row.col(|ui| {
                    right_aligned(
                        ui,
                        egui::RichText::new((index + 1).to_string())
                            .size(12.0)
                            .monospace()
                            .color(theme::MUTED),
                    );
                });
                row.col(|ui| {
                    ui.label(egui::RichText::new(def.label).size(12.5).color(theme::TEXT));
                    ui.label(
                        egui::RichText::new(def.symbol)
                            .size(10.5)
                            .monospace()
                            .color(theme::MUTED),
                    );
                });
                row.col(|ui| {
                    right_aligned(
                        ui,
                        egui::RichText::new(signed_delta(def.delta))
                            .size(12.0)
                            .monospace()
                            .color(theme::TEXT),
                    );
                });
                row.col(|ui| {
                    let is_target = index + 1 == solution.path.len() - 1;
                    let color = if is_target { theme::RED } else { theme::TEXT };
                    right_aligned(
                        ui,
                        egui::RichText::new(work.to_string())
                            .size(12.0)
                            .monospace()
                            .color(color),
                    );
                });
            });
        });
}

/// The save section: category/name controls and Save.
pub fn save_card(app: &mut ForgeApp, ui: &mut egui::Ui) {
    theme::section_title(ui, "Save");

    ui.label(
        egui::RichText::new("Category")
            .size(11.0)
            .color(theme::MUTED),
    );
    let options = ui::category_options(&app.recipes);
    ui.horizontal(|ui| {
        // The editable field and a compact dropdown of existing categories sit
        // on one row; picking an entry fills the field.
        let reserve = if options.is_empty() { 0.0 } else { 30.0 };
        let field_width = (ui.available_width() - reserve).max(80.0);
        ui.add(
            egui::TextEdit::singleline(&mut app.category)
                .desired_width(field_width)
                .min_size(vec2(0.0, theme::INPUT_HEIGHT))
                .hint_text("Category"),
        )
        .on_hover_text("Reusable material / family grouping");
        if !options.is_empty() {
            // A compact trigger (~30px) that opens a menu of existing category
            // names at their natural width. Picking one fills the field.
            ui.add_space(2.0);
            ui.menu_button("v", |ui| {
                for option in &options {
                    let selected = app.category.trim() == option;
                    if ui.selectable_label(selected, option).clicked() {
                        app.category = option.clone();
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text("Pick an existing category");
        }
    });

    ui.add_space(6.0);
    ui.label(egui::RichText::new("Name").size(11.0).color(theme::MUTED));
    ui.add(egui::TextEdit::singleline(&mut app.name).min_size(vec2(0.0, theme::INPUT_HEIGHT)))
        .on_hover_text("A name may be reused in another category");

    ui.add_space(8.0);
    let can_save = app.can_save();
    let button = egui::Button::new(egui::RichText::new("Save").strong().color(if can_save {
        Color32::WHITE
    } else {
        theme::MUTED
    }))
    .fill(if can_save {
        theme::ACCENT
    } else {
        theme::PANEL_2
    })
    .stroke(Stroke::new(
        1.0,
        if can_save { theme::ACCENT } else { theme::LINE },
    ))
    .min_size(vec2(90.0, 26.0));
    let response = ui.add_enabled(can_save, button);
    if response
        .on_hover_text("Save the current solution (Cmd/Ctrl+S; requires a valid name and solution)")
        .on_disabled_hover_text(if app.solution.is_none() {
            "No solution to save yet"
        } else {
            "Enter both a category and a name"
        })
        .clicked()
    {
        app.save_current();
    }

    match &app.feedback {
        Feedback::None => {}
        Feedback::Ok(message) => {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(message).size(12.0).color(theme::GREEN));
        }
        Feedback::Error(message) => {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(message).size(12.0).color(theme::RED));
        }
    }
}
