//! Native desktop entry point for the TFC Forge Planner.
//!
//! Startup order is deliberate and required for correctness:
//!   1. build exactly one multi-threaded Tokio runtime (never nested),
//!   2. open the SQLite recipe store on that runtime, which applies every
//!      pending migration before any query runs,
//!   3. load the recipe library,
//!   4. hand the loaded state to `eframe::run_native`.
//!
//! Any failure in steps 1-4 prints a clear message to stderr and exits with a
//! nonzero status. The app never starts as if it had persistence when it does
//! not.

mod app;
mod theme;
mod ui;

use std::process::ExitCode;

use tfc_forge::{RecipeStore, solve_forge};

use crate::app::ForgeApp;

/// App name used for the window title and eframe.
const APP_TITLE: &str = "TFC Forge Planner";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("TFC Forge Planner failed to start: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    // One runtime owns every database call. CRUD on user actions uses
    // `block_on` against this same handle; no second runtime is ever created.
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("could not create the async runtime: {error}"))?;

    // Open (which migrates) before the UI exists, so a migration failure is a
    // fatal startup error rather than a silently empty library.
    let store = runtime
        .block_on(RecipeStore::open_default())
        .map_err(|error| format!("could not open/migrate the recipe database: {error}"))?;

    let recipes = runtime
        .block_on(store.list())
        .map_err(|error| format!("could not load saved recipes: {error}"))?;

    let initial_solution = solve_forge(0, 100, &app::default_rules());

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_inner_size([1180.0, 820.0])
            .with_min_inner_size([760.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        APP_TITLE,
        native_options,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(ForgeApp::new(
                runtime,
                store,
                recipes,
                initial_solution,
            )))
        }),
    )
    .map_err(|error| format!("the native window could not start: {error}"))?;

    Ok(())
}
