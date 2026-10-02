//! Migration 3: remove the app-owned `default:*` recipes.
//!
//! Earlier builds seeded 24 bundled recipes whose ids all carry the reserved
//! `default:` prefix (see the history placeholder
//! `m20261002_000002_seed_recipes`). Those defaults are no longer bundled, so
//! this forward migration deletes every recipe with that prefix. The
//! `ON DELETE CASCADE` foreign keys on `recipe_rules` / `recipe_actions` remove
//! the child rows.
//!
//! Only the reserved id prefix is matched. Recipes the user created carry UUID
//! ids and are left untouched, even when their category and name happen to
//! match a former default - matching is strictly by id, never by category or
//! name.
//!
//! `down` is intentionally a no-op: the defaults are removed on purpose and the
//! seed asset that could recreate them is gone, so there is nothing meaningful
//! to restore.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_query::Expr;

/// Reserved id prefix used exclusively by app-owned default recipes.
const DEFAULT_ID_PREFIX: &str = "default:";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .exec_stmt(
                Query::delete()
                    .from_table(Recipes::Table)
                    .and_where(Expr::col(Recipes::Id).like(format!("{DEFAULT_ID_PREFIX}%")))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Intentionally a no-op: defaults are permanently removed.
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Recipes {
    Table,
    Id,
}
