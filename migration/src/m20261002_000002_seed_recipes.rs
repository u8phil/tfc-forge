//! Migration 2: historical seed placeholder (no longer seeds).
//!
//! This migration originally inserted the 24 bundled `default:*` recipes from
//! `assets/default_recipes.json`. The native app no longer ships default
//! recipes, so both `up` and `down` are now intentional no-ops.
//!
//! The module and migration name are retained verbatim because SeaORM records
//! applied migrations by name: already-upgraded user databases have this entry
//! in `seaql_migrations` and must not be re-run or have their migration history
//! broken. Do not reintroduce seeding here.
//!
//! Fresh databases get an empty recipe store from
//! `m20261002_000001_create_recipe_schema` alone, and databases that received
//! the former defaults have them removed by
//! `m20261002_000003_remove_default_recipes`.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // History placeholder: intentionally does not seed.
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // History placeholder: intentionally does not unseed. The former
        // defaults are purged by the forward migration, not by rollback.
        Ok(())
    }
}
