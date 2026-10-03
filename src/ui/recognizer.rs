//! Screenshot recognition panel: file chooser, drag/drop, paste, summary, apply.
//! Everything runs locally on the pixel buffer.

use tfc_forge::RecognitionResult;

use crate::app::{ForgeApp, choose_screenshot};
use crate::theme;

/// Render the recognizer section.
pub fn show(app: &mut ForgeApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    theme::section_title(ui, "Recognize");

    // Drag/drop is surfaced from eframe's raw input.
    let dropped: Option<Result<Vec<u8>, String>> = ctx.input(|input| {
        input
            .raw
            .dropped_files
            .first()
            .map(|file| file.bytes().map_err(|error| error.to_string()))
    });

    ui.horizontal(|ui| {
        if ui
            .button("Open image")
            .on_hover_text(
                "Open a native image chooser (Cmd/Ctrl+O). Formats: png, jpg, jpeg, gif, bmp, webp",
            )
            .clicked()
        {
            choose_screenshot(app);
        }
        if ui
            .button("Paste image")
            .on_hover_text(
                "Paste a pixel image or a copied image file from the clipboard (Cmd/Ctrl+V when no text field is focused)",
            )
            .clicked()
        {
            app.paste_clipboard_image();
        }
        ui.label(
            egui::RichText::new("Drop: not Wayland")
                .size(12.0)
                .color(theme::MUTED),
        );
    });

    if let Some(result) = dropped {
        match result {
            Ok(bytes) => {
                app.load_image_bytes(&bytes);
            }
            Err(error) => {
                app.recognition_error = Some(format!("Could not read dropped file: {error}"));
            }
        }
    }

    if let Some(error) = &app.recognition_error {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(error).size(12.0).color(theme::RED));
    }

    if let Some(result) = app.recognition.clone() {
        report(app, ui, &result);
    }
}

/// The compact two-row recognition summary and the apply control.
fn report(app: &mut ForgeApp, ui: &mut egui::Ui, result: &RecognitionResult) {
    let value =
        |value: Option<i32>| value.map_or_else(|| "-".to_string(), |value| value.to_string());
    let rules = if result.rules.is_empty() {
        "-".to_string()
    } else {
        result
            .rules
            .iter()
            .map(|rule| format!("{} {}", rule.rule_type.label(), rule.order.label()))
            .collect::<Vec<_>>()
            .join(" | ")
    };

    ui.add_space(8.0);
    egui::Grid::new("tfc-recognition-summary")
        .num_columns(2)
        .spacing([14.0, 6.0])
        .show(ui, |ui| {
            ui.label(format!("Current {}", value(result.start)));
            ui.label(format!("Target {}", value(result.target)));
            ui.end_row();
            ui.label("Rules");
            ui.label(rules);
            ui.end_row();
        });

    let can_apply = result.start.is_some() || result.target.is_some() || !result.rules.is_empty();
    ui.add_space(6.0);
    if ui
        .add_enabled(can_apply, egui::Button::new("Apply"))
        .on_hover_text("Only recognized fields are replaced; rules only when non-empty")
        .clicked()
    {
        let result = result.clone();
        app.apply_recognition(&result);
    }
}
