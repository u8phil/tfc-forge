//! Versioned schema and data migrations for the TFC forge planner.
//!
//! Migrations are registered in timestamp order. [`Migrator::up`] is invoked by
//! an application database open before any query, so every open applies all
//! pending migrations first.

pub use sea_orm_migration::prelude::*;

mod m20261002_000001_create_recipe_schema;
mod m20261002_000002_seed_recipes;
mod m20261002_000003_remove_default_recipes;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261002_000001_create_recipe_schema::Migration),
            Box::new(m20261002_000002_seed_recipes::Migration),
            Box::new(m20261002_000003_remove_default_recipes::Migration),
        ]
    }
}
