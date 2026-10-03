//! Central application state and the eframe update loop.
//!
//! The app owns:
//!   - exactly one Tokio runtime (passed in from `main`),
//!   - the [`RecipeStore`] handle,
//!   - the in-memory recipe list and all editor fields.
//!
//! Database calls are synchronous `block_on` against the already-owned runtime
//! and happen only in response to user actions, never per frame. No locks are
//! held across UI work and no second runtime is ever constructed.

use std::sync::Arc;

use tokio::runtime::Runtime;

use tfc_forge::{
    ActionId, ForgeRule, ForgeSolution, LIMIT, MIN_WORK, RecipeDraft, RecipeStore, RuleOrder,
    RuleType, SavedRecipe, solve_forge,
};

use crate::theme;
use crate::ui;

/// Default editor rules: Hit/Last + Bend/NotLast + Draw/NotLast.
pub fn default_rules() -> Vec<ForgeRule> {
    vec![
        ForgeRule::new(RuleType::Hit, RuleOrder::Last),
        ForgeRule::new(RuleType::Bend, RuleOrder::NotLast),
        ForgeRule::new(RuleType::Draw, RuleOrder::NotLast),
    ]
}

/// Transient feedback shown under the save section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Feedback {
    None,
    Ok(String),
    Error(String),
}

/// A profile of the rule set that is currently impossible to save/solve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleStatus {
    /// 1..=3 rules that can be assigned to distinct history positions.
    Valid,
    /// No rules at all.
    Empty,
    /// More than three rules (should be unreachable through the UI).
    TooMany,
    /// Impossible distinct-position assignment.
    ImpossibleSlots,
}

impl RuleStatus {
    pub fn of(rules: &[ForgeRule]) -> Self {
        if rules.is_empty() {
            RuleStatus::Empty
        } else if rules.len() > 3 {
            RuleStatus::TooMany
        } else if !tfc_forge::has_distinct_rule_order_slots(rules) {
            RuleStatus::ImpossibleSlots
        } else {
            RuleStatus::Valid
        }
    }

    pub fn is_valid(&self) -> bool {
        matches!(self, RuleStatus::Valid)
    }

    pub fn message(&self) -> Option<&'static str> {
        match self {
            RuleStatus::Valid => None,
            RuleStatus::Empty => Some("Add at least one rule to solve for a sequence."),
            RuleStatus::TooMany => Some("At most three rules are allowed."),
            RuleStatus::ImpossibleSlots => Some(
                "These rules cannot use distinct Last / Second Last / Third Last positions. \
                 Change an order to continue.",
            ),
        }
    }
}

/// The complete application.
pub struct ForgeApp {
    /// The single shared Tokio runtime.
    runtime: Arc<Runtime>,
    /// SQLite store handle.
    store: RecipeStore,

    // --- Editor state -----------------------------------------------------
    /// Current work value.
    pub start: i32,
    /// Target work value.
    pub target: i32,
    /// The 1..=3 forge rules.
    pub rules: Vec<ForgeRule>,
    /// Recipe category.
    pub category: String,
    /// Recipe name.
    pub name: String,

    /// Cached solution, recomputed whenever the input triple changes.
    pub solution: Option<ForgeSolution>,
    /// The exact `(start, target, rules)` triple the cached solution came from.
    solved_from: Option<(i32, i32, Vec<ForgeRule>)>,

    /// Transient save feedback.
    pub feedback: Feedback,

    // --- Library state ----------------------------------------------------
    /// Every saved recipe.
    pub recipes: Vec<SavedRecipe>,
    /// Library search query.
    pub search: String,
    /// Category filter (empty = all).
    pub category_filter: String,
    /// Currently selected recipe id.
    pub selected_id: Option<String>,
    /// Recipe id whose icon-only delete control is armed and awaiting a
    /// confirming second click. `None` when no delete is pending.
    pub pending_delete_id: Option<String>,
    /// Whether library search should receive focus on the next frame.
    pub focus_search: bool,
    /// Library error text.
    pub library_error: Option<String>,

    // --- Recognizer state -------------------------------------------------
    /// Last recognition result.
    pub recognition: Option<tfc_forge::RecognitionResult>,
    /// Recognizer status/error message.
    pub recognition_error: Option<String>,

    // --- Chrome -----------------------------------------------------------
    /// Whether the F1 shortcuts help window is open.
    pub show_help: bool,
}

impl ForgeApp {
    /// Construct the app with state loaded during startup.
    pub fn new(
        runtime: Runtime,
        store: RecipeStore,
        recipes: Vec<SavedRecipe>,
        initial_solution: Option<ForgeSolution>,
    ) -> Self {
        Self {
            runtime: Arc::new(runtime),
            store,
            start: 0,
            target: 100,
            rules: default_rules(),
            category: "Iron".to_string(),
            name: String::new(),
            solution: initial_solution,
            solved_from: Some((0, 100, default_rules())),
            feedback: Feedback::None,
            recipes,
            search: String::new(),
            category_filter: String::new(),
            selected_id: None,
            pending_delete_id: None,
            focus_search: false,
            library_error: None,
            recognition: None,
            recognition_error: None,
            show_help: false,
        }
    }

    /// Current rule validity.
    pub fn rule_status(&self) -> RuleStatus {
        RuleStatus::of(&self.rules)
    }

    /// Recompute the cached solution if any input changed since the last run.
    pub fn refresh_solution(&mut self) {
        let key = (self.start, self.target, self.rules.clone());
        if self.solved_from.as_ref() == Some(&key) {
            return;
        }
        self.solution = if self.rule_status().is_valid() {
            solve_forge(self.start, self.target, &self.rules)
        } else {
            None
        };
        self.solved_from = Some(key);
    }

    /// True when a save is currently possible.
    pub fn can_save(&self) -> bool {
        self.solution.is_some()
            && !self.category.trim().is_empty()
            && !self.name.trim().is_empty()
            && self.rule_status().is_valid()
    }

    /// Persist the current draft if valid. Runs on user action only.
    pub fn save_current(&mut self) {
        if self.category.trim().is_empty() || self.name.trim().is_empty() {
            self.feedback = Feedback::Error("Enter both a category and a recipe name.".to_string());
            return;
        }
        let Some(solution) = self.solution.clone() else {
            self.feedback = Feedback::Error("There is no solution to save yet.".to_string());
            return;
        };
        if !self.rule_status().is_valid() {
            self.feedback = Feedback::Error(
                self.rule_status()
                    .message()
                    .unwrap_or("Rules are not valid.")
                    .to_string(),
            );
            return;
        }

        let draft = RecipeDraft::from_parts(
            self.category.clone(),
            self.name.clone(),
            self.start,
            self.target,
            self.rules.clone(),
            solution.actions,
        );

        match self.runtime.block_on(self.store.save(draft)) {
            Ok(saved) => {
                let label = ui::saved_label(&saved);
                self.feedback = Feedback::Ok(format!("Saved \"{label}\"."));
                self.selected_id = Some(saved.id);
                self.reload_library();
            }
            Err(error) => {
                self.feedback = Feedback::Error(format!("Could not save: {error}"));
            }
        }
    }

    /// Load a saved recipe back into the editor.
    pub fn load_recipe(&mut self, recipe: &SavedRecipe) {
        // Loading any row is an unrelated action, so it backs out of a pending
        // delete confirmation.
        self.cancel_pending_delete();
        self.start = ui::clamp_work(recipe.start);
        self.target = ui::clamp_work(recipe.target);
        self.rules = recipe.rules.clone();
        self.category = recipe.category.clone();
        self.name = recipe.name.clone();
        self.selected_id = Some(recipe.id.clone());
        self.feedback = Feedback::None;
        self.refresh_solution();
    }

    /// Whether `id`'s delete control is armed and awaiting confirmation.
    pub fn is_delete_armed(&self, id: &str) -> bool {
        self.pending_delete_id.as_deref() == Some(id)
    }

    /// Handle one click on a row's icon-only delete control.
    ///
    /// The first click only arms `id` and returns `false`, so the caller must
    /// not touch the database. A second click on the same armed row returns
    /// `true` (the caller performs the delete) and clears the pending state.
    /// Clicking a different row arms that one instead, cancelling the previous
    /// confirmation.
    pub fn click_delete(&mut self, id: &str) -> bool {
        if self.is_delete_armed(id) {
            self.pending_delete_id = None;
            true
        } else {
            self.pending_delete_id = Some(id.to_string());
            false
        }
    }

    /// Drop any armed-but-unconfirmed delete, without touching the database.
    pub fn cancel_pending_delete(&mut self) {
        self.pending_delete_id = None;
    }

    /// Delete a recipe by id, then refresh the library.
    pub fn delete_recipe(&mut self, id: &str) {
        match self.runtime.block_on(self.store.delete(id)) {
            Ok(true) => {
                if self.selected_id.as_deref() == Some(id) {
                    self.selected_id = None;
                }
                self.library_error = None;
                self.feedback = Feedback::Ok("Recipe deleted.".to_string());
                self.reload_library();
            }
            Ok(false) => {
                self.library_error = Some("That recipe was already gone.".to_string());
                self.reload_library();
            }
            Err(error) => {
                self.library_error = Some(format!("Could not delete: {error}"));
            }
        }
        // Success or error, there is no longer a pending confirmation.
        self.pending_delete_id = None;
    }

    /// Re-read the full library from the store.
    pub fn reload_library(&mut self) {
        match self.runtime.block_on(self.store.list()) {
            Ok(recipes) => {
                self.recipes = recipes;
                self.library_error = None;
                // Keep the category filter pointing at a real category.
                if !self.category_filter.is_empty()
                    && !self
                        .recipes
                        .iter()
                        .any(|recipe| recipe.category == self.category_filter)
                {
                    self.category_filter.clear();
                }
            }
            Err(error) => {
                self.library_error = Some(format!("Could not load recipes: {error}"));
            }
        }
    }

    /// Apply recognized values, replacing only fields that were recognized.
    pub fn apply_recognition(&mut self, result: &tfc_forge::RecognitionResult) {
        if let Some(start) = result.start {
            self.start = ui::clamp_work(start);
        }
        if let Some(target) = result.target {
            self.target = ui::clamp_work(target);
        }
        if !result.rules.is_empty() && tfc_forge::has_distinct_rule_order_slots(&result.rules) {
            self.rules = result.rules.clone();
        }
        self.refresh_solution();
    }

    /// Simulate an action from the current work value.
    pub fn simulate(&mut self, action: ActionId) {
        let next = self.start + action.delta();
        if (MIN_WORK..=LIMIT).contains(&next) {
            self.start = next;
            self.refresh_solution();
        }
    }

    /// Follow an uploaded screenshot: decode and recognize.
    ///
    /// Returns `true` only when the bytes decoded into a valid RGBA buffer and
    /// recognition state was assigned. Every existing failure branch returns
    /// `false` after setting the same error text as before.
    pub fn load_image_bytes(&mut self, bytes: &[u8]) -> bool {
        match image::load_from_memory(bytes) {
            Ok(decoded) => {
                let rgba8 = decoded.to_rgba8();
                let (width, height) = rgba8.dimensions();
                match tfc_forge::RgbaImage::new(width as usize, height as usize, rgba8.into_raw()) {
                    Ok(rgba) => {
                        self.recognition = Some(tfc_forge::recognize_anvil(&rgba));
                        self.recognition_error = None;
                        true
                    }
                    Err(error) => {
                        self.recognition = None;
                        self.recognition_error = Some(format!("Invalid image buffer: {error}"));
                        false
                    }
                }
            }
            Err(error) => {
                self.recognition = None;
                self.recognition_error = Some(format!("Could not decode the image: {error}"));
                false
            }
        }
    }

    /// Read an image from a local filesystem path and load it for recognition.
    ///
    /// Returns `true` only when the bytes loaded successfully; a read failure
    /// keeps the existing `Could not read <path>: <error>` feedback and returns
    /// `false`.
    pub fn load_image_path(&mut self, path: &std::path::Path) -> bool {
        match std::fs::read(path) {
            Ok(bytes) => self.load_image_bytes(&bytes),
            Err(error) => {
                self.recognition = None;
                self.recognition_error =
                    Some(format!("Could not read {}: {error}", path.display()));
                false
            }
        }
    }

    /// Attempt to read an image from the system clipboard via arboard.
    ///
    /// Exactly one [`arboard::Clipboard`] is created per attempt. A real pixel
    /// image is preferred (`image/png` and the X11/Windows image formats). When
    /// the clipboard instead carries copied *files* - Wayland `text/uri-list`,
    /// X11 `URI_LIST`, Windows `CF_HDROP`, or macOS file URLs - `file_list()`
    /// maps them onto local paths and the first path that decodes as an image
    /// wins. If neither route yields an image, a concise visible error is
    /// surfaced instead of silently doing nothing.
    pub fn paste_clipboard_image(&mut self) {
        let mut board = match arboard::Clipboard::new() {
            Ok(board) => board,
            Err(error) => {
                self.recognition = None;
                self.recognition_error = Some(format!("No image on the clipboard: {error}"));
                return;
            }
        };

        // 1. Pixel image: retain the existing decoded-RGBA path exactly.
        let pixel_error = match board.get_image() {
            Ok(image) => {
                let expected = image.width.saturating_mul(image.height).saturating_mul(4);
                if image.bytes.len() != expected {
                    self.recognition = None;
                    self.recognition_error =
                        Some("Clipboard image buffer had an unexpected size.".to_string());
                    return;
                }
                match tfc_forge::RgbaImage::new(image.width, image.height, image.bytes.to_vec()) {
                    Ok(rgba) => {
                        self.recognition = Some(tfc_forge::recognize_anvil(&rgba));
                        self.recognition_error = None;
                    }
                    Err(error) => {
                        self.recognition = None;
                        self.recognition_error = Some(format!("Invalid clipboard image: {error}"));
                    }
                }
                return;
            }
            // No pixel image; fall through to copied files.
            Err(error) => error,
        };

        // 2. Copied files. `file_list()` is arboard's cross-platform mapping of
        //    Wayland `text/uri-list`, X11 `URI_LIST`, Windows `CF_HDROP`, and
        //    macOS file URLs onto local paths. arboard 3.6.1 splits the
        //    URI-list payload on `\n` without trimming `\r`, so a standard CRLF
        //    copy (Dolphin/GTK) leaves a trailing `\r` on each path; normalize
        //    every owned candidate before use. Load the first valid image, not
        //    merely the first path, and never panic on a bad candidate.
        let candidates = board.get().file_list().unwrap_or_default();
        let mut attempted = false;
        for path in candidates {
            let Some(path) = normalize_file_list_path(path) else {
                continue;
            };
            attempted = true;
            if self.load_image_path(&path) {
                return;
            }
        }

        // 3. Neither pixels nor any copied file produced an image. When at
        //    least one normalized candidate was actually attempted,
        //    `load_image_path` already left a specific read/decode error as the
        //    visible feedback; otherwise fall back to the pixel error so stale
        //    feedback is never retained.
        self.recognition = None;
        if let Some(error) = clipboard_no_image_error(attempted, &pixel_error) {
            self.recognition_error = Some(error);
        }
    }

    /// Render the complete UI. Called every frame.
    fn draw(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        handle_shortcuts(self, &ctx);
        self.refresh_solution();

        egui::CentralPanel::default_margins()
            .frame(egui::Frame::new().inner_margin(egui::Margin::same(14)))
            .show(root, |ui| {
                // Paint the real background gradient across the whole panel so
                // it resizes with the window.
                let rect = ui.max_rect().expand2(egui::vec2(14.0, 14.0));
                theme::paint_gradient(ui.painter(), rect);

                let wide = ui.available_width() >= 980.0;
                if wide {
                    // One vertical scroll region around both columns keeps the
                    // two-column alignment while letting tall content (notably
                    // the solution step table) be reached at the default window
                    // size instead of being clipped at the bottom.
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .id_salt("tfc-wide-scroll")
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                let gap = 14.0;
                                // The overlay scrollbar floats over the scroll
                                // region, so reserve its width as an explicit
                                // right gutter before splitting the panes. That
                                // keeps every right-pane control clear of the bar
                                // instead of ending underneath it.
                                let gutter = ui.spacing().scroll.bar_width
                                    + ui.spacing().scroll.bar_inner_margin;
                                // `horizontal_top` also inserts `item_spacing.x`
                                // between the two pane allocations, so fold that
                                // into the inter-pane gap; otherwise the extra
                                // spacing pushes the right pane into the gutter.
                                let inter_pane = gap + ui.spacing().item_spacing.x;
                                let (left_width, right_width) =
                                    wide_pane_widths(ui.available_width(), inter_pane, gutter);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(left_width, 0.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.set_width(left_width);
                                        crate::ui::anvil::show(self, ui);
                                        ui.add_space(14.0);
                                        crate::ui::solution::show(self, ui);
                                    },
                                );
                                ui.add_space(gap);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(right_width, 0.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.set_width(right_width);
                                        // Right pane order: recognize first,
                                        // then save, then the library last with
                                        // its adaptive list height.
                                        crate::ui::recognizer::show(self, &ctx, ui);
                                        ui.add_space(14.0);
                                        crate::ui::solution::save_card(self, ui);
                                        ui.add_space(14.0);
                                        crate::ui::library::show(self, ui);
                                    },
                                );
                            });
                        });
                } else {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            crate::ui::anvil::show(self, ui);
                            ui.add_space(14.0);
                            crate::ui::solution::show(self, ui);
                            ui.add_space(14.0);
                            // Same logical order as the wide right pane:
                            // recognize, save, library.
                            crate::ui::recognizer::show(self, &ctx, ui);
                            ui.add_space(14.0);
                            crate::ui::solution::save_card(self, ui);
                            ui.add_space(14.0);
                            crate::ui::library::show(self, ui);
                        });
                }
            });

        help_window(self, &ctx);
    }
}

/// Decide the visible error after a clipboard paste yields no image.
///
/// `attempted` is true when at least one normalized copied-file candidate was
/// handed to `load_image_path`, which then set a specific read/decode error that
/// must be preserved. When no candidate was attempted - no files at all, or
/// only candidates that normalized to `None` such as a bare `\r` - return the
/// generic pixel-read error so stale feedback is replaced.
fn clipboard_no_image_error(
    attempted: bool,
    pixel_error: &(impl std::fmt::Display + ?Sized),
) -> Option<String> {
    if attempted {
        None
    } else {
        Some(format!("No image on the clipboard: {pixel_error}"))
    }
}

/// Normalize one candidate from `arboard::Clipboard::file_list`.
///
/// arboard 3.6.1 maps a Wayland/X11 `text/uri-list` payload to paths by
/// splitting on `\n`, so a standard `\r\n` line terminator leaves a stray `\r`
/// at the end of every entry. Strip only trailing `\r` bytes and keep every
/// other byte, including spaces and arbitrary non-UTF-8 sequences. Return
/// `None` when nothing but `\r` bytes remain so the caller skips that
/// candidate. Non-Unix platforms are returned unchanged.
#[cfg(unix)]
fn normalize_file_list_path(path: std::path::PathBuf) -> Option<std::path::PathBuf> {
    use std::ffi::OsString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let mut bytes = path.as_os_str().as_bytes().to_vec();
    while bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    if bytes.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(OsString::from_vec(bytes)))
    }
}

/// Non-Unix platforms never hit arboard's `\n`-split URI-list parser, so the
/// path is returned unchanged; only an empty candidate is skipped.
#[cfg(not(unix))]
fn normalize_file_list_path(path: std::path::PathBuf) -> Option<std::path::PathBuf> {
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

impl eframe::App for ForgeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

/// Split the wide layout's total width into `(left, right)` pane widths,
/// reserving `gutter` on the right for the overlay scrollbar and `gap` between
/// the panes. `gap` is the full distance between the panes, including any
/// layout item spacing the caller has already folded in. The visible content
/// keeps the ~58/42 split of the pre-gutter content width, so the right pane
/// ends left of the scrollbar.
fn wide_pane_widths(total: f32, gap: f32, gutter: f32) -> (f32, f32) {
    let content = (total - gutter).max(1.0);
    let left = ((content - gap) * 0.58).max(360.0);
    let right = (content - gap - left).max(1.0);
    (left, right)
}

/// Handle global keyboard shortcuts using egui command semantics, so
/// Cmd/Ctrl is platform-correct.
fn handle_shortcuts(app: &mut ForgeApp, ctx: &egui::Context) {
    use egui::{Key, KeyboardShortcut, Modifiers};

    // F1 toggles help. Escape closes it.
    ctx.input_mut(|input| {
        if input.consume_key(Modifiers::NONE, Key::F1) {
            app.show_help = !app.show_help;
        }
        if app.show_help && input.consume_key(Modifiers::NONE, Key::Escape) {
            app.show_help = false;
        }
    });

    // Escape cancels an armed in-row delete confirmation. This runs before the
    // search-clear branch so the first Escape backs out of the pending delete
    // instead of touching the query.
    if app.pending_delete_id.is_some()
        && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
    {
        app.cancel_pending_delete();
    }

    let find = KeyboardShortcut::new(Modifiers::COMMAND, Key::F);
    let save = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
    let open = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);

    if ctx.input_mut(|input| input.consume_shortcut(&find)) {
        app.focus_search = true;
    }
    if ctx.input_mut(|input| input.consume_shortcut(&save)) {
        app.save_current();
    }
    if ctx.input_mut(|input| input.consume_shortcut(&open)) {
        choose_screenshot(app);
    }
    // Cmd/Ctrl+V: image paste. egui-winit drains the Ctrl+V press for any paste
    // attempt, so `consume_shortcut(COMMAND, V)` can never see it. It only emits
    // `Event::Paste` when the clipboard has text; for an image-only clipboard it
    // swallows the press silently but still forwards the V release, on which
    // Ctrl (and therefore `modifiers.command`) is still held because the normal
    // sequence releases V before Ctrl. Peek at the events immutably instead of
    // consuming them, so a focused TextEdit still receives its normal
    // `Event::Paste(String)` untouched. Only a focused text edit is exempt; the
    // explicit `Paste image` button bypasses this gate entirely. The clipboard
    // is touched only on an actual paste attempt, and `paste_clipboard_image`
    // owns the single Clipboard handle and the no-image feedback.
    let paste_attempt = ctx.input(|input| input.events.iter().any(is_paste_attempt));
    if paste_attempt && !ctx.text_edit_focused() {
        app.paste_clipboard_image();
    }

    // Escape clears the library search only when the search box itself owns
    // focus, help is closed, and there is a query to clear. Otherwise the key
    // is left untouched for whatever other widget wants it.
    let search_id = ui::library::search_id();
    let search_focused = ctx.memory(|memory| memory.has_focus(search_id));
    if !app.show_help
        && !app.search.is_empty()
        && search_focused
        && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
    {
        app.search.clear();
    }
}

/// True when this egui event is an image-paste attempt worth probing the
/// clipboard for.
///
/// Pinned to egui-winit 0.36.2: it consumes the Ctrl+V press for every paste
/// attempt and emits `Event::Paste` only when the clipboard carries text. An
/// image-only clipboard therefore produces no `Paste` event, but the matching V
/// release is still forwarded with the command modifier still set (V is
/// released before Ctrl). Accepting both keeps text paste working (the event is
/// never consumed) while making the image-only case observable.
fn is_paste_attempt(event: &egui::Event) -> bool {
    match event {
        egui::Event::Paste(_) => true,
        egui::Event::Key {
            key: egui::Key::V,
            pressed: false,
            modifiers,
            ..
        } => modifiers.command,
        _ => false,
    }
}

/// Open a native file chooser for an anvil screenshot.
pub fn choose_screenshot(app: &mut ForgeApp) {
    let picked = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
        .set_title("Choose an anvil screenshot")
        .pick_file();
    // Cancelling the chooser stays a no-op.
    if let Some(path) = picked {
        app.load_image_path(&path);
    }
}

/// The F1 shortcuts help window.
fn help_window(app: &mut ForgeApp, ctx: &egui::Context) {
    let mut open = app.show_help;
    egui::Window::new("Keyboard shortcuts")
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(egui::Stroke::new(1.0, theme::LINE))
                .inner_margin(egui::Margin::same(12)),
        )
        .show(ctx, |ui| {
            let rows = [
                ("Cmd/Ctrl + F", "Focus the library search box"),
                ("Cmd/Ctrl + S", "Save the current recipe"),
                ("Cmd/Ctrl + O", "Open a screenshot chooser"),
                ("Cmd/Ctrl + V", "Paste an image from the clipboard"),
                ("Escape", "Clear the library search when focused"),
                ("F1", "Toggle this shortcuts panel"),
                ("Escape", "Close this panel"),
            ];
            egui::Grid::new("shortcut-grid")
                .num_columns(2)
                .spacing([18.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    for (key, action) in rows {
                        ui.label(egui::RichText::new(key).monospace().color(theme::ACCENT));
                        ui.label(egui::RichText::new(action).color(theme::TEXT));
                        ui.end_row();
                    }
                });
        });
    app.show_help = open;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A UI smoke test: builds a real store on a temp path, constructs the app,
    /// installs the theme, and runs the complete update loop twice through a
    /// headless egui context. It must not panic for any panel or for the DB
    /// round trip, and it exercises the default solve.
    #[test]
    fn full_ui_renders_without_panicking() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let runtime = Runtime::new().expect("runtime");
        let store = runtime
            .block_on(RecipeStore::open(dir.path().join("recipes.sqlite3")))
            .expect("store");
        let recipes = runtime.block_on(store.list()).expect("list");
        let initial = solve_forge(0, 100, &default_rules());
        let mut app = ForgeApp::new(runtime, store, recipes, initial);

        egui::__run_test_ctx(|ctx| {
            theme::install(ctx);
            for _ in 0..2 {
                let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    app.draw(ui);
                });
                output.drop_without_applying_deltas();
            }
        });

        // Sanity: the default triple has a solution and a saved recipe request
        // goes through the same store path the UI uses.
        assert!(app.solution.is_some());
        app.category = "Smoke".to_string();
        app.name = "Test".to_string();
        app.save_current();
        assert!(
            matches!(app.feedback, Feedback::Ok(_)),
            "{:?}",
            app.feedback
        );
        app.reload_library();
        assert!(
            app.recipes
                .iter()
                .any(|recipe| recipe.category == "Smoke" && recipe.name == "Test")
        );
    }

    /// Regression for the review finding that Library rows hid their Load and
    /// Delete controls: with a saved recipe present, the full `draw` path (with
    /// its nested panels and scroll areas) must paint the row inside the
    /// visible viewport at the default 1180x820 window size. The row is now
    /// click-to-load and icon-only delete, so the old textual `Load` and
    /// `Delete` labels must no longer be rendered anywhere.
    #[test]
    fn library_row_paints_without_textual_load_or_delete() {
        use tfc_forge::{ActionId, SavedRecipe};
        let dir = tempfile::TempDir::new().expect("temp dir");
        let runtime = Runtime::new().expect("runtime");
        let store = runtime
            .block_on(RecipeStore::open(dir.path().join("recipes.sqlite3")))
            .expect("store");
        let actions = solve_forge(0, 85, &default_rules())
            .map(|solution| solution.actions)
            .unwrap_or_else(|| vec![ActionId::HitLight]);
        let recipe = SavedRecipe {
            id: "r1".to_string(),
            category: "Bronze".to_string(),
            name: "Plate".to_string(),
            start: 0,
            target: 85,
            rules: default_rules(),
            compact: tfc_forge::compact_actions(&actions),
            actions,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let initial = solve_forge(0, 100, &default_rules());
        let mut app = ForgeApp::new(runtime, store, vec![recipe], initial);

        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut texts = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1180.0, 820.0),
                )),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                app.draw(ui);
            });
            for shape in &output.shapes {
                collect_text(&shape.shape, &mut texts);
            }
            output.drop_without_applying_deltas();
        }
        let joined = texts.join("|");
        assert!(
            joined.contains("Bronze / Plate"),
            "row label missing: {joined}"
        );
        assert!(
            !joined.contains("Load"),
            "textual Load label should be gone: {joined}"
        );
        assert!(
            !joined.contains("Delete"),
            "textual Delete label should be gone: {joined}"
        );
    }

    /// The delete confirmation is a two-step state machine: the first click
    /// only arms, the second confirms, arming another row moves the
    /// confirmation, and loading a row cancels it.
    #[test]
    fn delete_confirmation_is_two_step_and_cancellable() {
        use tfc_forge::SavedRecipe;
        let dir = tempfile::TempDir::new().expect("temp dir");
        let runtime = Runtime::new().expect("runtime");
        let store = runtime
            .block_on(RecipeStore::open(dir.path().join("recipes.sqlite3")))
            .expect("store");
        let mut app = ForgeApp::new(runtime, store, Vec::new(), None);

        // First click arms without confirming or deleting.
        assert!(!app.click_delete("r1"));
        assert!(app.is_delete_armed("r1"));

        // Clicking a different row moves the arm, cancelling the previous one.
        assert!(!app.click_delete("r2"));
        assert!(!app.is_delete_armed("r1"));
        assert!(app.is_delete_armed("r2"));

        // An explicit cancel (the Escape path) drops the arm.
        app.cancel_pending_delete();
        assert!(!app.is_delete_armed("r2"));

        // The second click on an armed row confirms.
        assert!(!app.click_delete("r1"));
        assert!(app.click_delete("r1"));
        assert!(!app.is_delete_armed("r1"));

        // Loading a row also cancels a pending confirmation.
        let recipe = SavedRecipe {
            id: "r9".to_string(),
            category: "Iron".to_string(),
            name: "Plate".to_string(),
            start: 0,
            target: 10,
            rules: Vec::new(),
            compact: String::new(),
            actions: Vec::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        assert!(!app.click_delete("r9"));
        assert!(app.is_delete_armed("r9"));
        app.load_recipe(&recipe);
        assert!(!app.is_delete_armed("r9"));
    }

    /// The wide layout must reserve a right gutter for the overlay scrollbar so
    /// the right pane (and its hairlines and controls) ends left of the bar,
    /// while keeping the ~58/42 split and the shared gap.
    #[test]
    fn wide_layout_reserves_scrollbar_gutter() {
        let gutter = 14.0;
        let total = 1152.0;
        // The caller folds the layout's item spacing into the inter-pane gap,
        // so the widths must satisfy left + inter_pane + right == content.
        for inter_pane in [14.0_f32, 20.0] {
            let (left, right) = wide_pane_widths(total, inter_pane, gutter);
            assert!(right > 0.0);
            assert!(left + inter_pane + right + gutter <= total + f32::EPSILON);
            // The full gutter stays free on the right of the content.
            assert!((total - (left + inter_pane + right) - gutter).abs() < 0.01);
            // ~58/42 proportions of the content width (total minus gutter).
            let content = total - gutter;
            assert!((left - (content - inter_pane) * 0.58).abs() < 0.01);
            assert!((right - (content - inter_pane) * 0.42).abs() < 0.01);
        }
    }

    /// The image-paste signal must accept a real paste event and the command-V
    /// *release* egui-winit forwards after it swallows the image-paste press,
    /// while rejecting the swallowed press and an ordinary plain-V release.
    #[test]
    fn paste_attempt_predicate_matches_only_image_paste_signals() {
        use egui::{Event, Key, Modifiers};

        // A real paste event (text clipboard contents) counts as an attempt.
        assert!(is_paste_attempt(&Event::Paste("hello".to_string())));

        // Command-V release: egui-winit forwards this after swallowing the
        // image-paste press, with Ctrl still held.
        let command_v_release = Event::Key {
            key: Key::V,
            physical_key: Some(Key::V),
            pressed: false,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        assert!(is_paste_attempt(&command_v_release));

        // Command-V press: swallowed by egui-winit, egui never sees it, and if
        // it did it must not be treated as the paste signal.
        let command_v_press = Event::Key {
            key: Key::V,
            physical_key: Some(Key::V),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        assert!(!is_paste_attempt(&command_v_press));

        // Plain V release is ordinary typing, not a paste attempt.
        let plain_v_release = Event::Key {
            key: Key::V,
            physical_key: Some(Key::V),
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        assert!(!is_paste_attempt(&plain_v_release));
    }

    fn test_app() -> (tempfile::TempDir, ForgeApp) {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let runtime = Runtime::new().expect("runtime");
        let store = runtime
            .block_on(RecipeStore::open(dir.path().join("recipes.sqlite3")))
            .expect("store");
        (dir, ForgeApp::new(runtime, store, Vec::new(), None))
    }

    /// `load_image_path` reads a fixture, recognizes it, and reports success
    /// only when recognition state was actually assigned.
    #[test]
    fn load_image_path_recognizes_restored_fixture() {
        let (_dir, mut app) = test_app();
        let path = std::path::Path::new("assets/anvil-hit-draw-bend-t109.png");
        assert!(app.load_image_path(path));

        let result = app.recognition.as_ref().expect("recognition loaded");
        assert_eq!(result.target, Some(109));
        assert_eq!(result.rules.len(), 3);
        assert!(app.recognition_error.is_none());
    }

    /// Malformed bytes must return `false` and leave the existing decode error
    /// text, with no stale recognition state.
    #[test]
    fn load_image_bytes_failure_returns_false_and_sets_error() {
        let (_dir, mut app) = test_app();
        assert!(!app.load_image_bytes(b"not a real image"));
        assert!(app.recognition.is_none());
        let error = app.recognition_error.as_deref().unwrap_or("");
        assert!(
            error.starts_with("Could not decode the image:"),
            "unexpected error: {error}"
        );
    }

    /// A missing path must return `false`, clear recognition established by a
    /// previous load, and surface the `Could not read <path>: <error>` feedback.
    #[test]
    fn load_image_path_failure_clears_recognition_and_sets_error() {
        let (_dir, mut app) = test_app();
        let fixture = std::path::Path::new("assets/anvil-hit-draw-bend-t109.png");
        assert!(app.load_image_path(fixture));
        assert!(app.recognition.is_some());

        let missing = std::path::Path::new("assets/definitely-missing-image.png");
        assert!(!app.load_image_path(missing));
        assert!(app.recognition.is_none());
        let error = app.recognition_error.as_deref().unwrap_or("");
        assert!(error.starts_with("Could not read assets/"), "{error}");
        assert!(error.contains("definitely-missing-image.png"), "{error}");
    }

    /// arboard 3.6.1 maps a `text/uri-list` payload to paths by splitting on
    /// `\n` without trimming `\r`, so every standard CRLF entry keeps a trailing
    /// carriage return. The normalizer must strip only those trailing `\r`
    /// bytes, preserve every other byte (including arbitrary non-UTF-8 filename
    /// bytes), and reject a candidate that is empty afterwards.
    #[cfg(unix)]
    #[test]
    fn normalize_file_list_path_strips_trailing_carriage_return() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        // Standard CRLF `text/uri-list`: strip the trailing `\r`.
        let crlf = std::path::PathBuf::from(OsString::from_vec(
            b"/tmp/anvil-hit-draw-bend-t109.png\r".to_vec(),
        ));
        assert_eq!(
            normalize_file_list_path(crlf),
            Some(std::path::PathBuf::from(
                "/tmp/anvil-hit-draw-bend-t109.png"
            ))
        );

        // A path without `\r` is unchanged, spaces and all.
        let plain = std::path::PathBuf::from("/tmp/anvil hit draw bend.png");
        assert_eq!(normalize_file_list_path(plain.clone()), Some(plain));

        // Arbitrary non-UTF-8 path bytes survive normalization untouched.
        let non_utf8 =
            std::path::PathBuf::from(OsString::from_vec(b"/tmp/\xff\xfe-anvil.png\r".to_vec()));
        assert_eq!(
            normalize_file_list_path(non_utf8),
            Some(std::path::PathBuf::from(OsString::from_vec(
                b"/tmp/\xff\xfe-anvil.png".to_vec()
            )))
        );

        // Nothing but `\r` left after normalization is rejected.
        let only_cr = std::path::PathBuf::from(OsString::from_vec(b"\r".to_vec()));
        assert_eq!(normalize_file_list_path(only_cr), None);
    }

    /// The clipboard failure decision must fall back to the generic pixel error
    /// when no normalized candidate was attempted (no files at all, or only
    /// candidates that normalized to `None`), and must preserve the specific
    /// per-path read/decode error when one was attempted.
    #[test]
    fn clipboard_no_image_error_only_generic_without_attempt() {
        assert_eq!(
            clipboard_no_image_error(false, "no image data"),
            Some("No image on the clipboard: no image data".to_string())
        );
        assert_eq!(clipboard_no_image_error(true, "no image data"), None);
    }

    /// The recognizer panel must render the input controls plus the compact
    /// two-row fixture summary, and none of the removed preview/confidence/
    /// detail labels, captured from the real headless draw output.
    #[test]
    fn ui_text_includes_compact_recognition_summary() {
        let (_dir, mut app) = test_app();
        assert!(app.load_image_path(std::path::Path::new("assets/anvil-hit-draw-bend-t109.png")));
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut texts = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1180.0, 820.0),
                )),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                app.draw(ui);
            });
            for shape in &output.shapes {
                collect_text(&shape.shape, &mut texts);
            }
            output.drop_without_applying_deltas();
        }
        let joined = texts.join("|");
        for label in [
            "Open image",
            "Paste image",
            "Drop: not Wayland",
            "Current",
            "Target",
            "Rules",
            "109",
            "Hit Last | Draw Second Last | Bend Third Last",
        ] {
            assert!(joined.contains(label), "missing {label:?}: {joined}");
        }
        assert!(
            !joined.contains("Panel"),
            "Panel label should be gone: {joined}"
        );
        assert!(
            !joined.contains("Per-slot detail"),
            "Per-slot detail should be gone: {joined}"
        );
    }

    fn collect_text(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text(shape, out);
                }
            }
            _ => {}
        }
    }
}
