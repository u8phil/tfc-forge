//! Native Rust port of the TerraFirmaCraft forge planner.
//!
//! Stage 1 provides the foundation:
//!   - [`domain`]: the faithful forge domain model (types, constants, validation)
//!   - [`solver`]: the BFS shortest-path solver
//!   - [`db`]: SQLite recipe persistence built on SeaORM + SeaQuery, with
//!     versioned [`tfc_forge_migration`] migrations applied on every open.
//!
//! Stage 2 adds:
//!   - [`recognition`]: the local, UI-independent anvil screenshot recognizer.
//!
//! Stage 3 adds the native desktop application on top of this library. The
//! egui screens live in the binary crate (`src/main.rs`, `src/app.rs`,
//! `src/theme.rs`, `src/ui/`) and are intentionally private so the domain,
//! solver, store and recognizer stay independent of any UI framework.

pub mod db;
pub mod domain;
pub mod recognition;
pub mod solver;

pub use db::{RecipeStore, StoreError};
pub use domain::{
    ACTIONS, ActionDef, ActionId, ForgeRule, ForgeSolution, LIMIT, MIN_WORK, RULE_ORDERS,
    RULE_TYPES, RecipeDraft, RuleOrder, RuleType, SavedRecipe, ValidationError, action_def,
    compact_actions, has_distinct_rule_order_slots, path_from_actions,
};
pub use recognition::{
    FieldConfidence, PanelLocation, RecognitionError, RecognitionResult, RgbaImage, RuleField,
    recognize_anvil,
};
pub use solver::{rule_satisfied, rules_satisfied, solve_forge, solve_forge_bounded};

// Re-export the migrator so downstream binaries can inspect/migrate explicitly.
pub use tfc_forge_migration::Migrator;
