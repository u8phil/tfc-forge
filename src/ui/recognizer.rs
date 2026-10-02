//! Screenshot recognition panel: file chooser, drag/drop, paste, preview,
//! confidence report, and apply. Everything runs locally on the pixel buffer.

use egui::{Color32, Stroke, vec2};

use tfc_forge::{RecognitionResult, RuleField};

use crate::app::{ForgeApp, choose_screenshot};
use crate::theme;

/// Render the recognizer section.
pub fn show(app: &mut ForgeApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    theme::section_title(ui, "Recognize");

    // Drag/drop is surfaced from eframe's raw input.
    let dropped: Vec<(String, Result<Vec<u8>, String>)> = ctx.input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .map(|file| {
                (
                    file.path().display().to_string(),
                    file.bytes().map_err(|error| error.to_string()),
                )
            })
            .collect()
    });

    ui.horizontal(|ui| {
        if ui
            .button("Open image")
            .on_hover_text(
                "Open a native image chooser (Cmd/Ctrl+O). Formats: png, jpg, jpeg, gif, bmp, webp",
            )
            .clicked()
        {
            choose_screenshot(app, ctx);
        }
        ui.label(
            egui::RichText::new("Drop / Paste")
                .size(12.0)
                .color(theme::MUTED),
        );
    });

    if let Some((source, result)) = dropped.into_iter().next() {
        match result {
            Ok(bytes) => app.load_image_bytes(ctx, &bytes, source),
            Err(error) => {
                app.recognition_error = Some(format!("Could not read dropped file: {error}"));
            }
        }
    }

    if let Some(error) = &app.recognition_error {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(error).size(12.0).color(theme::RED));
    }

    if let Some(image) = &app.image {
        ui.add_space(8.0);
        let (width, height) = (image.rgba.width as f32, image.rgba.height as f32);
        let max_width = ui.available_width().min(420.0);
        let scale = (max_width / width).min(1.0);
        let preview_size = vec2(width * scale, height * scale);
        ui.image((image.texture.id(), preview_size))
            .on_hover_text(format!(
                "{} ({}x{})",
                image.source, image.rgba.width, image.rgba.height
            ));
    }

    if let Some(result) = app.recognition.clone() {
        report(app, ui, &result);
    }
}

/// The recognition report with confidence data, rules, warnings, and apply.
fn report(app: &mut ForgeApp, ui: &mut egui::Ui, result: &RecognitionResult) {
    ui.add_space(8.0);
    egui::Grid::new("tfc-recognition-grid")
        .num_columns(2)
        .spacing([14.0, 6.0])
        .show(ui, |ui| {
            confidence_cell(
                ui,
                "Current",
                result
                    .start
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                result.confidence.start,
            );
            confidence_cell(
                ui,
                "Target",
                result
                    .target
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                result.confidence.target,
            );
            ui.end_row();
            confidence_cell(
                ui,
                "Rules",
                result.rules.len().to_string(),
                result.confidence.rules,
            );
            confidence_cell(
                ui,
                "Panel",
                result
                    .panel
                    .as_ref()
                    .map(|panel| format!("{},{} x{}", panel.x, panel.y, panel.scale))
                    .unwrap_or_else(|| "not found".to_string()),
                result.confidence.panel,
            );
            ui.end_row();
        });

    ui.add_space(6.0);
    if result.rules.is_empty() {
        ui.label(
            egui::RichText::new("No rules recognized.")
                .size(12.0)
                .color(theme::MUTED),
        );
    } else {
        for (index, rule) in result.rules.iter().enumerate() {
            ui.label(
                egui::RichText::new(format!(
                    "Rule {}: {} - {}",
                    index + 1,
                    rule.rule_type.label(),
                    rule.order.label()
                ))
                .size(12.0)
                .color(theme::TEXT),
            );
        }
    }

    // Per-slot detail.
    if !result.rule_fields.is_empty() {
        ui.add_space(4.0);
        ui.collapsing("Per-slot detail", |ui| {
            for field in &result.rule_fields {
                slot_detail(ui, field);
            }
        });
    }

    if !result.warnings.is_empty() {
        ui.add_space(4.0);
        for warning in &result.warnings {
            ui.label(egui::RichText::new(warning).size(11.5).color(theme::RED));
            ui.add_space(2.0);
        }
    }

    let can_apply = result.start.is_some() || result.target.is_some() || !result.rules.is_empty();
    ui.add_space(6.0);
    let button = egui::Button::new(egui::RichText::new("Apply").strong().color(if can_apply {
        Color32::WHITE
    } else {
        theme::MUTED
    }))
    .fill(if can_apply {
        theme::ACCENT
    } else {
        theme::PANEL_2
    })
    .stroke(Stroke::new(
        1.0,
        if can_apply {
            theme::ACCENT
        } else {
            theme::LINE
        },
    ));
    if ui
        .add_enabled(can_apply, button)
        .on_hover_text("Only recognized fields are replaced; rules only when non-empty")
        .clicked()
    {
        let result = result.clone();
        app.apply_recognition(&result);
    }
}

/// A labeled value with a thin confidence bar underneath.
fn confidence_cell(ui: &mut egui::Ui, key: &str, value: String, confidence: f64) {
    ui.vertical(|ui| {
        ui.set_width(170.0);
        ui.label(
            egui::RichText::new(key.to_uppercase())
                .size(10.0)
                .color(theme::MUTED),
        );
        ui.label(
            egui::RichText::new(value)
                .size(14.0)
                .strong()
                .color(theme::TEXT),
        );
        let fraction = confidence.clamp(0.0, 1.0) as f32;
        let (bar, _) = ui.allocate_exact_size(vec2(160.0, 4.0), egui::Sense::hover());
        ui.painter().rect_filled(bar, 0.0, theme::LINE);
        let filled = egui::Rect::from_min_size(bar.min, vec2(bar.width() * fraction, bar.height()));
        ui.painter().rect_filled(filled, 0.0, theme::ACCENT);
    });
}

/// One rule slot's recognized type/order and per-field confidence.
fn slot_detail(ui: &mut egui::Ui, field: &RuleField) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!("Slot {}", field.slot + 1))
                .size(11.5)
                .strong()
                .color(theme::TEXT),
        );
        let type_text = field
            .rule_type
            .map(|rule_type| rule_type.label().to_string())
            .unwrap_or_else(|| "-".to_string());
        let order_text = field
            .order
            .map(|order| order.label().to_string())
            .unwrap_or_else(|| "-".to_string());
        ui.label(
            egui::RichText::new(format!(
                "type {type_text} ({:.0}%) | order {order_text} ({:.0}%)",
                field.type_confidence * 100.0,
                field.order_confidence * 100.0
            ))
            .size(11.5)
            .color(theme::MUTED),
        );
        if let Some(note) = &field.note {
            ui.label(egui::RichText::new(note).size(11.0).color(theme::RED));
        }
    });
}
