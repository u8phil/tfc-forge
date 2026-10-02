//! The anvil workbench: rule controls, the 2x4 action grid, and the 0..150
//! work track on one flat surface.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, vec2};

use tfc_forge::{
    ActionId, ForgeRule, LIMIT, MIN_WORK, RULE_ORDERS, RULE_TYPES, RuleOrder, RuleType, action_def,
};

use crate::app::{ForgeApp, RuleStatus};
use crate::theme;
use crate::ui;

/// The in-game 2x4 action grid order. Row 1: Light, Medium, Punch, Bend.
/// Row 2: Hard, Draw, Upset, Shrink.
const ACTION_GRID: [[ActionId; 4]; 2] = [
    [
        ActionId::HitLight,
        ActionId::HitMedium,
        ActionId::Punch,
        ActionId::Bend,
    ],
    [
        ActionId::HitHard,
        ActionId::Draw,
        ActionId::Upset,
        ActionId::Shrink,
    ],
];

/// Preferred width of one rule slot (glyph + order + delete). At the app's
/// minimum width up to three of these plus the gaps still fit, but the helpers
/// shrink them to the available span when it is narrower.
const RULE_SLOT_WIDTH: f32 = 170.0;
/// Gap between two adjacent rule slots.
const RULE_SLOT_GAP: f32 = 12.0;
/// Compact inner margin between a slot backdrop and its controls.
const RULE_FRAME_MARGIN: f32 = 8.0;
/// The 1px neutral border drawn around a slot backdrop.
const RULE_FRAME_STROKE: f32 = 1.0;
/// Side of the clickable rule-type square.
const RULE_TYPE_SIZE: f32 = 46.0;
/// Height of the clickable order ladder.
const RULE_ORDER_HEIGHT: f32 = 36.0;
/// Vertical gap between the type row and the order control.
const RULE_ROW_GAP: f32 = 6.0;
/// Full height of a rule slot backdrop: its body plus the inner margins and
/// the 1px border on each side.
const RULE_SLOT_HEIGHT: f32 = RULE_TYPE_SIZE
    + RULE_ROW_GAP
    + RULE_ORDER_HEIGHT
    + (RULE_FRAME_MARGIN + RULE_FRAME_STROKE) * 2.0;

/// Single-letter glyph for a rule type.
fn rule_glyph(rule_type: RuleType) -> &'static str {
    match rule_type {
        RuleType::Hit => "H",
        RuleType::Draw => "D",
        RuleType::Punch => "P",
        RuleType::Bend => "B",
        RuleType::Upset => "U",
        RuleType::Shrink => "S",
    }
}

/// Render the whole anvil workbench section.
pub fn show(app: &mut ForgeApp, ui: &mut egui::Ui) {
    theme::section_title(ui, "Rules");
    rule_strip(app, ui);

    ui.add_space(14.0);
    theme::section_title(ui, "Actions");
    action_grid(app, ui);

    ui.add_space(14.0);
    theme::section_title(ui, "Work");
    work_track(app, ui);
}

/// The compact rule strip: one flat backdrop per rule enclosing its type,
/// order and delete controls, with the whole 1..=3-slot group centered in the
/// available width and the add control centered beneath it.
fn rule_strip(app: &mut ForgeApp, ui: &mut egui::Ui) {
    let mut remove_index: Option<usize> = None;
    let mut add_rule = false;
    let rule_count = app.rules.len();
    let (slot_width, group_width) = rule_slot_widths(ui.available_width(), rule_count);

    // `allocate_ui_with_layout` at the exact group width is what keeps the
    // whole row centered: the slots themselves never grow beyond it.
    ui.vertical_centered(|ui| {
        ui.allocate_ui_with_layout(
            vec2(group_width, RULE_SLOT_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.x = RULE_SLOT_GAP;
                for index in 0..rule_count {
                    let rule = app.rules[index];
                    rule_slot(ui, slot_width, |ui| {
                        ui.horizontal(|ui| {
                            if rule_glyph_button(ui, index, rule.rule_type).clicked() {
                                // Cycle to the next rule type. Rule type
                                // changes never affect order-slot validity.
                                app.rules[index].rule_type = ui::cycle(&RULE_TYPES, rule.rule_type);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                let can_remove = rule_count > 1;
                                let button = ui.add_enabled(
                                    can_remove,
                                    egui::Button::new(egui::RichText::new("x").color(theme::MUTED))
                                        .min_size(vec2(22.0, 22.0)),
                                );
                                if button
                                    .on_hover_text("Remove this rule")
                                    .on_disabled_hover_text("At least one rule is required")
                                    .clicked()
                                {
                                    remove_index = Some(index);
                                }
                            });
                        });

                        // Order control: click cycles to the next order that
                        // keeps the whole set assignable to distinct positions.
                        let order = rule.order;
                        let response = order_button(ui, index, order);
                        if response.clicked() {
                            let candidate = ui::cycle_until(&RULE_ORDERS, order, |candidate| {
                                let mut next = app.rules.clone();
                                next[index].order = candidate;
                                tfc_forge::has_distinct_rule_order_slots(&next)
                            });
                            app.rules[index].order = candidate;
                        }
                    });
                }
            },
        );
    });

    if let Some(index) = remove_index {
        app.rules.remove(index);
    }

    ui.add_space(8.0);
    let add = ui.vertical_centered(|ui| {
        ui.add_enabled(
            rule_count < 3,
            egui::Button::new(
                egui::RichText::new(format!("+ Add rule ({rule_count}/3)")).color(
                    if rule_count < 3 {
                        theme::TEXT
                    } else {
                        theme::MUTED
                    },
                ),
            ),
        )
    });
    if add
        .inner
        .on_hover_text("Add another forge rule")
        .on_disabled_hover_text("Three rules is the maximum")
        .clicked()
    {
        add_rule = true;
    }

    if add_rule {
        app.rules
            .push(ForgeRule::new(RuleType::Hit, RuleOrder::Any));
    }

    // Live validity feedback for impossible rule sets.
    match RuleStatus::of(&app.rules) {
        RuleStatus::Valid => {}
        status => {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(status.message().unwrap_or(""))
                    .size(12.0)
                    .color(theme::RED),
            );
        }
    }
}

/// One flat square rule backdrop enclosing that rule's type, order and delete
/// controls. The fill and border are neutral theme tokens and the corners are
/// square; it is the only frame around those controls.
fn rule_slot(ui: &mut egui::Ui, slot_width: f32, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::PANEL_2)
        .stroke(Stroke::new(1.0, theme::LINE))
        .inner_margin(egui::Margin::same(RULE_FRAME_MARGIN as i8))
        .show(ui, |ui| {
            // The row-to-order gap is driven by the layout spacing so the slot
            // height stays independent of the ambient theme spacing.
            ui.spacing_mut().item_spacing.y = RULE_ROW_GAP;
            // Keep the painted outer rect equal to `slot_width`: the frame adds
            // its inner margin and 1px border on each side. Reserving the inner
            // size explicitly is what pins the frame width.
            let chrome = (RULE_FRAME_MARGIN + RULE_FRAME_STROKE) * 2.0;
            let inner = vec2(
                (slot_width - chrome).max(1.0),
                (RULE_SLOT_HEIGHT - chrome).max(1.0),
            );
            ui.allocate_ui_with_layout(inner, egui::Layout::top_down(egui::Align::Min), |ui| {
                body(ui)
            });
        });
}

/// The shared `(slot_width, group_width)` for `rule_count` equal rule slots in
/// the `available` span. The preferred width is three full slots plus their
/// gaps; a narrower span shrinks every slot equally so the whole group fits
/// without horizontal overflow.
fn rule_slot_widths(available: f32, rule_count: usize) -> (f32, f32) {
    let count = rule_count.max(1) as f32;
    let gaps = RULE_SLOT_GAP * (count - 1.0);
    let preferred = RULE_SLOT_WIDTH * count + gaps;
    let group_width = preferred.min(available.max(1.0));
    let slot_width = ((group_width - gaps) / count).max(1.0);
    (slot_width, group_width)
}

/// The clickable rule-type square.
fn rule_glyph_button(ui: &mut egui::Ui, index: usize, rule_type: RuleType) -> egui::Response {
    let size = vec2(RULE_TYPE_SIZE, RULE_TYPE_SIZE);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let stroke = if response.hovered() {
        Stroke::new(1.0, theme::ACCENT)
    } else {
        Stroke::new(1.0, theme::LINE)
    };
    painter.rect_filled(rect, 0.0, theme::FIELD_BG);
    painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Inside);
    painter.text(
        rect.center() - vec2(0.0, 6.0),
        Align2::CENTER_CENTER,
        rule_glyph(rule_type),
        FontId::proportional(20.0),
        theme::TEXT,
    );
    painter.text(
        rect.center() + vec2(0.0, 11.0),
        Align2::CENTER_CENTER,
        rule_type.label(),
        FontId::proportional(9.5),
        theme::MUTED,
    );
    response.on_hover_text(format!(
        "Rule {} action: {}. Click to cycle.",
        index + 1,
        rule_type.label()
    ))
}

/// The clickable order control, including the three-rung ladder.
fn order_button(ui: &mut egui::Ui, index: usize, order: RuleOrder) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), RULE_ORDER_HEIGHT),
        Sense::click(),
    );
    let painter = ui.painter();
    let stroke = if response.hovered() {
        Stroke::new(1.0, theme::ACCENT)
    } else {
        Stroke::new(1.0, theme::LINE)
    };
    painter.rect_filled(rect, 0.0, theme::FIELD_BG);
    painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Inside);

    // Three-rung ladder: top = third-last, mid = second-last, bottom = last.
    let ladder = ladder_state(order);
    let rung_x = rect.left() + 12.0;
    for (rung, on) in ladder.iter().enumerate() {
        let y = rect.top() + 8.0 + rung as f32 * 7.0;
        let rung_rect = Rect::from_min_size(Pos2::new(rung_x, y), vec2(18.0, 3.0));
        let color = if *on { theme::ACCENT } else { theme::LINE };
        painter.rect_filled(rung_rect, 0.0, color);
    }

    painter.text(
        Pos2::new(rect.left() + 42.0, rect.center().y - 6.0),
        Align2::LEFT_CENTER,
        "Order",
        FontId::proportional(9.0),
        theme::MUTED,
    );
    painter.text(
        Pos2::new(rect.left() + 42.0, rect.center().y + 7.0),
        Align2::LEFT_CENTER,
        order.label(),
        FontId::proportional(12.5),
        theme::TEXT,
    );

    response.on_hover_text(format!(
        "Rule {} order: {}. Click to cycle (unavailable combinations are skipped).",
        index + 1,
        order.label()
    ))
}

/// The three ladder rungs for an order: `[third_last, second_last, last]`.
fn ladder_state(order: RuleOrder) -> [bool; 3] {
    match order {
        RuleOrder::Any => [true, true, true],
        RuleOrder::NotLast => [true, true, false],
        RuleOrder::Last => [false, false, true],
        RuleOrder::SecondLast => [false, true, false],
        RuleOrder::ThirdLast => [true, false, false],
    }
}

/// Preferred action-button width and the shared per-row grid span for the
/// available width. Four buttons plus three 6px gaps make up `full_width`,
/// which is the preferred 126px-button span capped to the available width so a
/// narrow layout shrinks instead of overflowing.
fn action_grid_widths(available: f32) -> (f32, f32) {
    const BUTTON_WIDTH: f32 = 126.0;
    const GAP: f32 = 6.0;
    let preferred = BUTTON_WIDTH * 4.0 + GAP * 3.0;
    let full_width = preferred.min(available);
    let button_width = (full_width - GAP * 3.0) / 4.0;
    (button_width, full_width)
}

/// The exact 2x4 in-game action grid, centered within the Actions span. Both
/// rows are allocated at the same `full_width`, so egui's centered layout gives
/// them identical left and right free space.
fn action_grid(app: &mut ForgeApp, ui: &mut egui::Ui) {
    const ROW_HEIGHT: f32 = 46.0;
    let (button_width, full_width) = action_grid_widths(ui.available_width());

    let mut clicked: Option<ActionId> = None;
    ui.vertical_centered(|ui| {
        for row in ACTION_GRID {
            ui.allocate_ui_with_layout(
                vec2(full_width, ROW_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    for action in row {
                        let result = action_button(app.start, button_width, ROW_HEIGHT, action, ui);
                        if result.clicked() {
                            clicked = Some(action);
                        }
                    }
                },
            );
        }
    });

    if let Some(action) = clicked {
        app.simulate(action);
    }
}

/// A single action button, disabled when the move would leave 0..=150.
fn action_button(
    current: i32,
    width: f32,
    height: f32,
    action: ActionId,
    ui: &mut egui::Ui,
) -> egui::Response {
    let next = current + action.delta();
    let enabled = (MIN_WORK..=LIMIT).contains(&next);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let painter = ui.painter();

    let hovered = response.hovered() && enabled;
    let focused = response.has_focus() && enabled;
    // Every action is framed against the surface: a strong neutral border at
    // rest and a clear blue border on hover or keyboard focus.
    let (fill, stroke) = if !enabled {
        (theme::FIELD_BG, Stroke::new(1.5, theme::LINE_SOFT))
    } else if hovered || focused {
        (theme::SURFACE_3, Stroke::new(1.5, theme::ACCENT))
    } else {
        (theme::PANEL_2, Stroke::new(1.5, theme::LINE))
    };

    painter.rect_filled(rect, 0.0, fill);
    painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Inside);

    let text_color = if enabled { theme::TEXT } else { theme::MUTED };
    painter.text(
        Pos2::new(rect.left() + 8.0, rect.center().y - 6.0),
        Align2::LEFT_CENTER,
        action_def(action).label,
        FontId::proportional(12.5),
        text_color,
    );
    let delta_text = if action.delta() >= 0 {
        format!("+{}  {}", action.delta(), action.symbol())
    } else {
        format!("{}  {}", action.delta(), action.symbol())
    };
    painter.text(
        Pos2::new(rect.left() + 8.0, rect.center().y + 7.0),
        Align2::LEFT_CENTER,
        delta_text,
        FontId::monospace(10.5),
        theme::MUTED,
    );

    if enabled {
        response.on_hover_text(format!(
            "{}: set current work from {} to {}",
            action_def(action).label,
            current,
            next
        ))
    } else {
        response.on_hover_text(format!(
            "{} would move work to {}, outside 0..{}",
            action_def(action).label,
            next,
            LIMIT
        ))
    }
}

/// The horizontal 0..150 work track plus the compact integer controls. The two
/// standalone sliders are gone; the Current and Target DragValues sit together
/// on one row.
fn work_track(app: &mut ForgeApp, ui: &mut egui::Ui) {
    track_widget(app, ui);

    ui.add_space(10.0);
    let mut start = app.start;
    let mut target = app.target;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Current")
                .size(11.5)
                .color(theme::GREEN),
        );
        ui.scope(|ui| {
            ui.spacing_mut().interact_size.y = theme::INPUT_HEIGHT;
            ui.add(
                egui::DragValue::new(&mut start)
                    .range(MIN_WORK..=LIMIT)
                    .speed(1.0),
            );
        });
        ui.add_space(18.0);
        ui.label(egui::RichText::new("Target").size(11.5).color(theme::RED));
        ui.scope(|ui| {
            ui.spacing_mut().interact_size.y = theme::INPUT_HEIGHT;
            ui.add(
                egui::DragValue::new(&mut target)
                    .range(MIN_WORK..=LIMIT)
                    .speed(1.0),
            );
        });
    });
    app.start = ui::clamp_work(start);
    app.target = ui::clamp_work(target);
}

/// The bespoke horizontal track with independently draggable green/red markers.
///
/// There is no interaction on the track as a whole. Each marker owns a stable
/// egui id plus a flag-sized hit rect, so a drag can only start on a flag and,
/// once captured, keeps updating that same marker in x regardless of pointer y
/// until release.
fn track_widget(app: &mut ForgeApp, ui: &mut egui::Ui) {
    let height = 56.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let painter = ui.painter();

    let rail_left = rect.left() + 6.0;
    let rail_right = rect.right() - 6.0;
    let rail_width = (rail_right - rail_left).max(1.0);
    let rail_y = rect.top() + 22.0;
    let rail_height = 8.0;
    let rail = Rect::from_min_size(Pos2::new(rail_left, rail_y), vec2(rail_width, rail_height));
    painter.rect_filled(rail, 0.0, theme::INSET);
    painter.rect_stroke(rail, 0.0, Stroke::new(1.0, theme::LINE), StrokeKind::Inside);

    let value_x = |value: i32| rail_left + (value as f32 / LIMIT as f32) * rail_width;

    // Range band between current and target.
    let left = app.start.min(app.target);
    let right = app.start.max(app.target);
    let range_rect = Rect::from_min_size(
        Pos2::new(value_x(left), rail_y + 1.0),
        vec2((value_x(right) - value_x(left)).max(0.0), rail_height - 2.0),
    );
    if range_rect.width() > 0.5 {
        painter.rect_filled(range_rect, 0.0, theme::ACCENT_DIM);
    }

    // Ticks at 0,25,...,150 and their labels.
    for tick in 0..=6 {
        let value = tick * 25;
        let x = value_x(value);
        painter.line_segment(
            [
                Pos2::new(x, rail.bottom()),
                Pos2::new(x, rail.bottom() + 4.0),
            ],
            Stroke::new(1.0, theme::LINE),
        );
        painter.text(
            Pos2::new(x, rect.bottom() - 2.0),
            Align2::CENTER_BOTTOM,
            value.to_string(),
            FontId::monospace(9.5),
            theme::MUTED,
        );
    }

    // Markers. Current sits on the above-rail lane and target on the below-rail
    // lane, so they stay distinguishable when their values coincide. Each flag
    // is drawn, then given its own hit area and stable id.
    let current_tip = rail_y - 5.0;
    let target_tip = rail_y + 13.0;
    draw_marker(
        painter,
        value_x(app.start),
        current_tip,
        MarkerLane::Above,
        theme::GREEN,
    );
    draw_marker(
        painter,
        value_x(app.target),
        target_tip,
        MarkerLane::Below,
        theme::RED,
    );

    let lane_mid = rail.center().y;
    let current_hit = marker_hit_rect(value_x(app.start), current_tip, MarkerLane::Above, lane_mid);
    let target_hit = marker_hit_rect(value_x(app.target), target_tip, MarkerLane::Below, lane_mid);

    let current_response = ui.interact(current_hit, current_marker_id(), Sense::click_and_drag());
    let target_response = ui.interact(target_hit, target_marker_id(), Sense::click_and_drag());

    if let Some(value) = dragged_value(&current_response, rail_left, rail_width) {
        app.start = value;
    }
    if let Some(value) = dragged_value(&target_response, rail_left, rail_width) {
        app.target = value;
    }
}

/// Stable id for the green current-work marker flag.
fn current_marker_id() -> egui::Id {
    egui::Id::new("tfc-work-current-marker")
}

/// Stable id for the red target-work marker flag.
fn target_marker_id() -> egui::Id {
    egui::Id::new("tfc-work-target-marker")
}

/// Convert a pointer x within the rail to an integer work value strict in
/// `MIN_WORK..=LIMIT`. Positions at or beyond either edge clamp to that edge.
fn value_from_x(pointer_x: f32, rail_left: f32, rail_width: f32) -> i32 {
    if rail_width <= 0.0 {
        return MIN_WORK;
    }
    let fraction = ((pointer_x - rail_left) / rail_width).clamp(0.0, 1.0);
    (fraction * LIMIT as f32)
        .clamp(MIN_WORK as f32, LIMIT as f32)
        .round() as i32
}

/// The value a marker press/drag resolves to, if that marker is active.
///
/// Only the pointer's x is consulted, so an active drag keeps updating its own
/// marker even when the pointer crosses into the other marker's lane.
fn dragged_value(response: &egui::Response, rail_left: f32, rail_width: f32) -> Option<i32> {
    if (response.dragged() || response.clicked())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        Some(value_from_x(pointer.x, rail_left, rail_width))
    } else {
        None
    }
}

/// The clickable hit rect around a marker's own flag. It covers only that
/// marker's lane half, split at `lane_mid`, so the current and target hit areas
/// never overlap even when their values coincide.
fn marker_hit_rect(x: f32, tip_y: f32, lane: MarkerLane, lane_mid: f32) -> Rect {
    let half_width = 11.0;
    let (top, bottom) = match lane {
        MarkerLane::Above => (tip_y - 14.0, lane_mid),
        MarkerLane::Below => (lane_mid, tip_y + 14.0),
    };
    Rect::from_min_max(
        Pos2::new(x - half_width, top),
        Pos2::new(x + half_width, bottom),
    )
}

/// Which side of the rail a marker's body occupies. The apex always approaches
/// the rail, so this selects the triangle orientation without consulting any
/// absolute screen coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkerLane {
    /// Green current marker: body above the rail, apex pointing down at it.
    Above,
    /// Red target marker: body below the rail, apex pointing up at it.
    Below,
}

/// The three triangle corners for a marker at `(x, tip_y)` on `lane`.
fn marker_points(x: f32, tip_y: f32, lane: MarkerLane, size: f32) -> [Pos2; 3] {
    match lane {
        // Above the rail: body corners sit above the apex, which points down.
        MarkerLane::Above => [
            Pos2::new(x - size, tip_y - size * 1.4),
            Pos2::new(x, tip_y),
            Pos2::new(x + size, tip_y - size * 1.4),
        ],
        // Below the rail: body corners sit below the apex, which points up.
        MarkerLane::Below => [
            Pos2::new(x - size, tip_y + size * 1.4),
            Pos2::new(x, tip_y),
            Pos2::new(x + size, tip_y + size * 1.4),
        ],
    }
}

/// Draw a thin triangle marker; the numeric control above carries the value, so
/// no value bubble is drawn on the track.
fn draw_marker(painter: &egui::Painter, x: f32, tip_y: f32, lane: MarkerLane, color: Color32) {
    let size = 6.0;
    let points = marker_points(x, tip_y, lane, size).to_vec();
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The marker triangle must point at the rail from its lane and keep its
    /// body on the correct side, regardless of absolute screen position.
    #[test]
    fn marker_triangles_point_at_the_rail_from_their_lane() {
        let (x, tip_y, size) = (50.0, 400.0, 6.0);
        let above = marker_points(x, tip_y, MarkerLane::Above, size);
        let below = marker_points(x, tip_y, MarkerLane::Below, size);

        // Above: apex at tip_y, body corners strictly above (smaller y).
        assert_eq!(above[1], Pos2::new(x, tip_y));
        assert!(above[0].y < tip_y && above[2].y < tip_y);

        // Below: apex at tip_y, body corners strictly below (larger y).
        assert_eq!(below[1], Pos2::new(x, tip_y));
        assert!(below[0].y > tip_y && below[2].y > tip_y);
    }

    /// Wide spans keep 126px buttons; narrower spans use the full width and
    /// shrink every button so the four-column row fits without overflow.
    #[test]
    fn action_grid_width_is_capped_and_buttons_shrink_to_fit() {
        // Preferred span: 4 * 126 + 3 * 6.
        assert_eq!(action_grid_widths(1000.0), (126.0, 522.0));
        // Exactly at the preferred width: no shrink.
        assert_eq!(action_grid_widths(522.0), (126.0, 522.0));
        // Narrower: span is the full available width and the rendered row
        // (buttons + gaps) matches it exactly.
        let (button, full) = action_grid_widths(400.0);
        assert_eq!(full, 400.0);
        assert!((button * 4.0 + 6.0 * 3.0 - full).abs() < 0.001);
    }

    /// Pointer x maps to a work value that is always clamped to `0..=150`,
    /// including positions outside the rail.
    #[test]
    fn marker_value_clamps_to_the_rail_bounds() {
        assert_eq!(value_from_x(-500.0, 10.0, 100.0), 0);
        assert_eq!(value_from_x(10.0, 10.0, 100.0), 0);
        assert_eq!(value_from_x(35.0, 10.0, 100.0), 38);
        assert_eq!(value_from_x(60.0, 10.0, 100.0), 75);
        assert_eq!(value_from_x(110.0, 10.0, 100.0), 150);
        assert_eq!(value_from_x(9999.0, 10.0, 100.0), 150);
        // A degenerate rail never panics and stays in range.
        assert_eq!(value_from_x(50.0, 10.0, 0.0), 0);
    }

    /// The current and target flags use distinct stable ids and never overlap,
    /// even when their values coincide, so each can be grabbed independently.
    #[test]
    fn markers_have_independent_ids_and_hit_areas() {
        assert_ne!(current_marker_id(), target_marker_id());

        // Coincident x: the two hit rects still occupy their own lane half.
        let lane_mid = 26.0;
        let current = marker_hit_rect(50.0, 17.0, MarkerLane::Above, lane_mid);
        let target = marker_hit_rect(50.0, 35.0, MarkerLane::Below, lane_mid);
        assert!(current.max.y <= target.min.y);

        // Each flag tip is inside its own hit rect.
        assert!(current.contains(Pos2::new(50.0, 17.0)));
        assert!(target.contains(Pos2::new(50.0, 35.0)));
        // Space away from a flag (a different lane, or empty rail x) is not.
        assert!(!current.contains(Pos2::new(50.0, 35.0)));
        assert!(!target.contains(Pos2::new(50.0, 17.0)));
        assert!(!current.contains(Pos2::new(500.0, 17.0)));
        assert!(!target.contains(Pos2::new(500.0, 35.0)));
    }

    /// Rule slots share one width and shrink together to fit the available
    /// span, so a 3-slot group never exceeds even the minimum app width.
    #[test]
    fn rule_slot_widths_share_one_width_and_fit_the_span() {
        // Preferred: 3 * 170 + 2 * 12.
        assert_eq!(rule_slot_widths(1000.0, 3), (170.0, 534.0));
        // Exactly at the preferred width: no shrink.
        assert_eq!(rule_slot_widths(534.0, 3), (170.0, 534.0));
        // Narrower span: fills it exactly and every slot stays equal.
        let (slot, group) = rule_slot_widths(400.0, 3);
        assert_eq!(group, 400.0);
        assert!((slot * 3.0 + RULE_SLOT_GAP * 2.0 - group).abs() < 0.001);
        // The minimum 760px window leaves ~732px after the central-panel
        // margins, and the preferred group still fits there.
        let (slot, group) = rule_slot_widths(732.0, 3);
        assert_eq!((slot, group), (170.0, 534.0));
        assert!(group <= 732.0);
        // A single rule keeps its full preferred width even in a wide span.
        assert_eq!(rule_slot_widths(1000.0, 1), (170.0, 170.0));
    }

    /// Every painted rectangle (with its fill) and text (with its position),
    /// flattened out of egui's nested shape list.
    fn collect_rects_and_texts(
        shapes: &[egui::epaint::Shape],
    ) -> (Vec<egui::epaint::RectShape>, Vec<(Pos2, String)>) {
        fn walk(
            shape: &egui::epaint::Shape,
            rects: &mut Vec<egui::epaint::RectShape>,
            texts: &mut Vec<(Pos2, String)>,
        ) {
            match shape {
                egui::epaint::Shape::Rect(rect) => rects.push(rect.clone()),
                egui::epaint::Shape::Text(text) => {
                    texts.push((text.pos, text.galley.text().to_string()));
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, rects, texts);
                    }
                }
                _ => {}
            }
        }
        let mut rects = Vec::new();
        let mut texts = Vec::new();
        for shape in shapes {
            walk(shape, &mut rects, &mut texts);
        }
        (rects, texts)
    }

    /// The three rule backdrops are centered as one group, share equal widths
    /// and gaps, and each one encloses its type, order and delete controls.
    #[test]
    fn rule_backdrops_are_centered_and_enclose_their_controls() {
        use tfc_forge::RecipeStore;
        use tokio::runtime::Runtime;

        let dir = tempfile::TempDir::new().expect("temp dir");
        let runtime = Runtime::new().expect("runtime");
        let store = runtime
            .block_on(RecipeStore::open(dir.path().join("recipes.sqlite3")))
            .expect("store");
        let mut app = ForgeApp::new(runtime, store, Vec::new(), None);
        assert_eq!(app.rules.len(), 3);

        let width = 732.0;
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 820.0));
        let ctx = egui::Context::default();
        theme::install(&ctx);

        let mut shapes = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                rule_strip(&mut app, ui);
            });
            shapes = output
                .shapes
                .iter()
                .map(|clipped| clipped.shape.clone())
                .collect();
            output.drop_without_applying_deltas();
        }

        let (rects, texts) = collect_rects_and_texts(&shapes);
        let (slot_width, _) = rule_slot_widths(width, 3);

        // Backdrops: the PANEL_2 / LINE rectangles at the slot size.
        let mut backdrops: Vec<egui::Rect> = rects
            .iter()
            .filter(|rect| {
                rect.fill == theme::PANEL_2
                    && rect.stroke.color == theme::LINE
                    && (rect.stroke.width - RULE_FRAME_STROKE).abs() < 0.001
                    && (rect.rect.width() - slot_width).abs() < 0.5
                    && (rect.rect.height() - RULE_SLOT_HEIGHT).abs() < 0.5
            })
            .map(|rect| rect.rect)
            .collect();
        backdrops.sort_by(|a, b| a.min.x.partial_cmp(&b.min.x).unwrap());
        assert_eq!(
            backdrops.len(),
            3,
            "expected three rule backdrops, found {}",
            backdrops.len()
        );

        // Centered as one group.
        let left_free = backdrops[0].min.x - screen.min.x;
        let right_free = screen.max.x - backdrops[2].max.x;
        assert!(
            (left_free - right_free).abs() < 1.0,
            "group not centered: left_free={left_free} right_free={right_free}"
        );
        // Equal widths and consistent gaps.
        for pair in backdrops.windows(2) {
            let gap = pair[1].min.x - pair[0].max.x;
            assert!(
                (gap - RULE_SLOT_GAP).abs() < 0.5,
                "inconsistent slot gap: {gap}"
            );
        }

        // Each backdrop encloses the type glyph, the order control and the
        // delete control (text and painted control rectangles).
        for backdrop in &backdrops {
            let labels: Vec<&str> = texts
                .iter()
                .filter(|(pos, _)| backdrop.contains(*pos))
                .map(|(_, text)| text.as_str())
                .collect();
            assert!(
                labels.contains(&"Order"),
                "order control missing inside {backdrop:?}: {labels:?}"
            );
            assert!(
                labels.contains(&"x"),
                "delete control missing inside {backdrop:?}: {labels:?}"
            );
            assert!(
                labels
                    .iter()
                    .any(|label| matches!(*label, "H" | "D" | "P" | "B" | "U" | "S")),
                "type glyph missing inside {backdrop:?}: {labels:?}"
            );

            let enclosed: Vec<egui::Rect> = rects
                .iter()
                .filter(|rect| backdrop.contains_rect(rect.rect))
                .map(|rect| rect.rect)
                .collect();
            assert!(
                enclosed.iter().any(|rect| {
                    (rect.width() - RULE_TYPE_SIZE).abs() < 0.5
                        && (rect.height() - RULE_TYPE_SIZE).abs() < 0.5
                }),
                "type control not enclosed by {backdrop:?}"
            );
            assert!(
                enclosed
                    .iter()
                    .any(|rect| (rect.height() - RULE_ORDER_HEIGHT).abs() < 0.5),
                "order control not enclosed by {backdrop:?}"
            );
            assert!(
                enclosed
                    .iter()
                    .any(|rect| rect.width() < 30.0 && rect.height() < 30.0),
                "delete control not enclosed by {backdrop:?}"
            );
        }
    }
}
