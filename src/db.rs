//! SQLite recipe persistence.
//!
//! Application CRUD is written as direct [`sea_query`] statements executed
//! through SeaORM's SQLite connection. Schema creation and seeding live in the
//! [`tfc_forge_migration`] workspace crate; every open runs
//! [`Migrator::up`] before any query.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr, ExprTrait, QueryResult,
    TransactionTrait, sea_query::Alias,
};
use sea_query::{Expr, Order, Query};

use tfc_forge_migration::{Migrator, MigratorTrait};

use crate::domain::{
    ActionId, ForgeRule, RecipeDraft, RuleOrder, RuleType, SavedRecipe, ValidationError,
    compact_actions, parse_saved_recipe_value, path_from_actions,
};

/// Database file name under the OS config directory.
const DB_FILE_NAME: &str = "recipes.sqlite3";
/// Application config directory segment.
const APP_DIR: &str = "tfc-forge";

/// Errors returned by [`RecipeStore`].
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] DbErr),
    #[error("could not resolve a configuration directory for this system")]
    NoConfigDir,
    #[error("recipe validation failed: {0}")]
    Validation(#[from] ValidationError),
    #[error("stored recipe {0} is malformed")]
    Corrupt(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

/// Column idents for the `recipes` table.
mod recipes {
    pub const TABLE: &str = "recipes";
    pub const ID: &str = "id";
    pub const CATEGORY: &str = "category";
    pub const CATEGORY_KEY: &str = "category_key";
    pub const NAME: &str = "name";
    pub const NAME_KEY: &str = "name_key";
    pub const START: &str = "start";
    pub const TARGET: &str = "target";
    pub const COMPACT: &str = "compact";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const RULES: &str = "recipe_rules";
    pub const ACTIONS: &str = "recipe_actions";
    pub const RECIPE_ID: &str = "recipe_id";
    pub const POSITION: &str = "position";
    pub const RULE_TYPE: &str = "rule_type";
    pub const RULE_ORDER: &str = "rule_order";
    pub const ACTION_ID: &str = "action_id";
}

/// A SeaORM-backed SQLite recipe store.
///
/// Cheap to clone: the inner [`DatabaseConnection`] is a connection pool handle.
#[derive(Clone)]
pub struct RecipeStore {
    db: DatabaseConnection,
    path: PathBuf,
}

impl RecipeStore {
    /// Open (creating if needed) the default database at
    /// `dirs::config_dir()/tfc-forge/recipes.sqlite3`, creating the directory
    /// and applying all pending migrations.
    pub async fn open_default() -> Result<Self, StoreError> {
        let dir = dirs::config_dir()
            .ok_or(StoreError::NoConfigDir)?
            .join(APP_DIR);
        std::fs::create_dir_all(&dir)?;
        Self::open(dir.join(DB_FILE_NAME)).await
    }

    /// Open (creating if needed) a database at an explicit path, applying all
    /// pending migrations before returning.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        let url = format!("sqlite://{}?mode=rwc", path.display());
        let mut options = ConnectOptions::new(url);
        options
            .max_connections(5)
            .connect_timeout(Duration::from_secs(10))
            .map_sqlx_sqlite_opts(|sqlite| {
                sqlite
                    .foreign_keys(true)
                    .busy_timeout(Duration::from_secs(5))
                    .journal_mode(sqlx_journal_mode_wal())
            });

        let db = Database::connect(options).await?;
        Migrator::up(&db, None).await?;
        Ok(Self { db, path })
    }

    /// Path of the underlying SQLite file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reference to the underlying SeaORM connection.
    pub fn connection(&self) -> &DatabaseConnection {
        &self.db
    }

    /// List every saved recipe, ordered stably by category then name
    /// (case-insensitive), with child rules/actions loaded in position order.
    pub async fn list(&self) -> Result<Vec<SavedRecipe>, StoreError> {
        let backend = self.db.get_database_backend();
        let statement = Query::select()
            .columns([
                recipes::ID,
                recipes::CATEGORY,
                recipes::NAME,
                recipes::START,
                recipes::TARGET,
                recipes::COMPACT,
                recipes::CREATED_AT,
                recipes::UPDATED_AT,
            ])
            .from(Alias::new(recipes::TABLE))
            .order_by(Alias::new(recipes::CATEGORY_KEY), Order::Asc)
            .order_by(Alias::new(recipes::NAME_KEY), Order::Asc)
            .to_owned();

        let rows = self.db.query_all_raw(backend.build(&statement)).await?;

        let mut base = Vec::with_capacity(rows.len());
        for row in &rows {
            base.push(BaseRecipe {
                id: row.try_get("", recipes::ID)?,
                category: row.try_get("", recipes::CATEGORY)?,
                name: row.try_get("", recipes::NAME)?,
                start: row.try_get("", recipes::START)?,
                target: row.try_get("", recipes::TARGET)?,
                compact: row.try_get("", recipes::COMPACT)?,
                created_at: row.try_get("", recipes::CREATED_AT)?,
                updated_at: row.try_get("", recipes::UPDATED_AT)?,
            });
        }

        let ids: Vec<String> = base.iter().map(|recipe| recipe.id.clone()).collect();
        let rules = self.load_rules(&ids).await?;
        let actions = self.load_actions(&ids).await?;

        let mut recipes = Vec::with_capacity(base.len());
        for recipe in base {
            let recipe_rules = rules
                .get(&recipe.id)
                .cloned()
                .ok_or_else(|| StoreError::Corrupt(recipe.id.clone()))?;
            // A recipe whose start equals its target is legal with no actions,
            // so a missing entry means an empty action list rather than
            // corruption. Rules remain mandatory (persisted recipes require
            // 1..=3), so a missing rules entry is still corrupt.
            let recipe_actions = actions.get(&recipe.id).cloned().unwrap_or_default();
            recipes.push(SavedRecipe {
                id: recipe.id,
                category: recipe.category,
                name: recipe.name,
                start: recipe.start,
                target: recipe.target,
                rules: recipe_rules,
                actions: recipe_actions,
                compact: recipe.compact,
                created_at: recipe.created_at,
                updated_at: recipe.updated_at,
            });
        }
        Ok(recipes)
    }

    /// Validate and upsert a draft.
    ///
    /// Upsert is case-insensitive on `(category, name)`: an existing recipe in
    /// the same category with the same name (ignoring case) is updated in place,
    /// preserving `created_at` and refreshing `updated_at`. The same name in a
    /// different category is a distinct recipe.
    pub async fn save(&self, draft: RecipeDraft) -> Result<SavedRecipe, StoreError> {
        draft.validate()?;

        let category = draft.category.trim().to_string();
        let name = draft.name.trim().to_string();
        let category_key = category.to_lowercase();
        let name_key = name.to_lowercase();
        let compact = draft.compact();

        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let existing = self
            .find_existing(&category_key, &name_key)
            .await?
            .unwrap_or_else(|| Existing {
                id: uuid::Uuid::new_v4().to_string(),
                created_at: now.clone(),
            });

        let txn = self.db.begin().await?;

        self.delete_children(&txn, &existing.id).await?;

        let backend = txn.get_database_backend();
        let upsert = Query::insert()
            .into_table(Alias::new(recipes::TABLE))
            .columns([
                Alias::new(recipes::ID),
                Alias::new(recipes::CATEGORY),
                Alias::new(recipes::CATEGORY_KEY),
                Alias::new(recipes::NAME),
                Alias::new(recipes::NAME_KEY),
                Alias::new(recipes::START),
                Alias::new(recipes::TARGET),
                Alias::new(recipes::COMPACT),
                Alias::new(recipes::CREATED_AT),
                Alias::new(recipes::UPDATED_AT),
            ])
            .values_panic([
                existing.id.clone().into(),
                category.clone().into(),
                category_key.clone().into(),
                name.clone().into(),
                name_key.clone().into(),
                draft.start.into(),
                draft.target.into(),
                compact.clone().into(),
                existing.created_at.clone().into(),
                now.clone().into(),
            ])
            .on_conflict(
                sea_query::OnConflict::columns([
                    Alias::new(recipes::CATEGORY_KEY),
                    Alias::new(recipes::NAME_KEY),
                ])
                .update_columns([
                    recipes::ID,
                    recipes::CATEGORY,
                    recipes::NAME,
                    recipes::START,
                    recipes::TARGET,
                    recipes::COMPACT,
                    recipes::UPDATED_AT,
                ])
                .to_owned(),
            )
            .to_owned();
        txn.execute_raw(backend.build(&upsert)).await?;

        let final_row = txn
            .query_one_raw(
                backend.build(
                    &Query::select()
                        .columns([recipes::ID, recipes::CREATED_AT, recipes::UPDATED_AT])
                        .from(Alias::new(recipes::TABLE))
                        .and_where(Expr::col(Alias::new(recipes::CATEGORY_KEY)).eq(category_key))
                        .and_where(Expr::col(Alias::new(recipes::NAME_KEY)).eq(name_key))
                        .to_owned(),
                ),
            )
            .await?
            .ok_or_else(|| StoreError::Corrupt(existing.id.clone()))?;
        let id: String = final_row.try_get("", recipes::ID)?;
        let created_at: String = final_row.try_get("", recipes::CREATED_AT)?;
        let updated_at: String = final_row.try_get("", recipes::UPDATED_AT)?;

        for (position, rule) in draft.rules.iter().enumerate() {
            let insert = Query::insert()
                .into_table(Alias::new(recipes::RULES))
                .columns([
                    Alias::new(recipes::RECIPE_ID),
                    Alias::new(recipes::POSITION),
                    Alias::new(recipes::RULE_TYPE),
                    Alias::new(recipes::RULE_ORDER),
                ])
                .values_panic([
                    id.clone().into(),
                    (position as i32).into(),
                    rule.rule_type.as_str().into(),
                    rule.order.as_str().into(),
                ])
                .to_owned();
            txn.execute_raw(backend.build(&insert)).await?;
        }

        for (position, action) in draft.actions.iter().enumerate() {
            let insert = Query::insert()
                .into_table(Alias::new(recipes::ACTIONS))
                .columns([
                    Alias::new(recipes::RECIPE_ID),
                    Alias::new(recipes::POSITION),
                    Alias::new(recipes::ACTION_ID),
                ])
                .values_panic([
                    id.clone().into(),
                    (position as i32).into(),
                    action.as_str().into(),
                ])
                .to_owned();
            txn.execute_raw(backend.build(&insert)).await?;
        }

        txn.commit().await?;

        Ok(SavedRecipe {
            id,
            category,
            name,
            start: draft.start,
            target: draft.target,
            rules: draft.rules,
            actions: draft.actions,
            compact,
            created_at,
            updated_at,
        })
    }

    /// Delete a recipe by id. Returns `true` when a row was removed.
    ///
    /// Child rows are removed explicitly (rather than relying solely on the
    /// cascade) so the operation is portable across SQLite builds that were not
    /// compiled with foreign key enforcement.
    pub async fn delete(&self, id: &str) -> Result<bool, StoreError> {
        let backend = self.db.get_database_backend();
        let txn = self.db.begin().await?;

        let remove_rules = Query::delete()
            .from_table(Alias::new(recipes::RULES))
            .and_where(Expr::col(Alias::new(recipes::RECIPE_ID)).eq(id))
            .to_owned();
        txn.execute_raw(backend.build(&remove_rules)).await?;

        let remove_actions = Query::delete()
            .from_table(Alias::new(recipes::ACTIONS))
            .and_where(Expr::col(Alias::new(recipes::RECIPE_ID)).eq(id))
            .to_owned();
        txn.execute_raw(backend.build(&remove_actions)).await?;

        let remove_recipe = Query::delete()
            .from_table(Alias::new(recipes::TABLE))
            .and_where(Expr::col(Alias::new(recipes::ID)).eq(id))
            .to_owned();
        let result = txn.execute_raw(backend.build(&remove_recipe)).await?;

        txn.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    /// Find the id/created_at of an existing recipe by canonical key.
    async fn find_existing(
        &self,
        category_key: &str,
        name_key: &str,
    ) -> Result<Option<Existing>, StoreError> {
        let backend = self.db.get_database_backend();
        let statement = Query::select()
            .columns([recipes::ID, recipes::CREATED_AT])
            .from(Alias::new(recipes::TABLE))
            .and_where(Expr::col(Alias::new(recipes::CATEGORY_KEY)).eq(category_key))
            .and_where(Expr::col(Alias::new(recipes::NAME_KEY)).eq(name_key))
            .to_owned();
        let row = self.db.query_one_raw(backend.build(&statement)).await?;
        match row {
            Some(row) => Ok(Some(Existing {
                id: row.try_get("", recipes::ID)?,
                created_at: row.try_get("", recipes::CREATED_AT)?,
            })),
            None => Ok(None),
        }
    }

    async fn delete_children<C>(&self, conn: &C, id: &str) -> Result<(), DbErr>
    where
        C: ConnectionTrait,
    {
        let backend = conn.get_database_backend();
        for table in [recipes::RULES, recipes::ACTIONS] {
            let statement = Query::delete()
                .from_table(Alias::new(table))
                .and_where(Expr::col(Alias::new(recipes::RECIPE_ID)).eq(id))
                .to_owned();
            conn.execute_raw(backend.build(&statement)).await?;
        }
        Ok(())
    }

    async fn load_rules(
        &self,
        ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<ForgeRule>>, StoreError> {
        let mut map: std::collections::HashMap<String, Vec<ForgeRule>> =
            std::collections::HashMap::new();
        if ids.is_empty() {
            return Ok(map);
        }
        let backend = self.db.get_database_backend();
        let statement = Query::select()
            .columns([recipes::RECIPE_ID, recipes::RULE_TYPE, recipes::RULE_ORDER])
            .from(Alias::new(recipes::RULES))
            .and_where(Expr::col(Alias::new(recipes::RECIPE_ID)).is_in(ids.iter().cloned()))
            .order_by(Alias::new(recipes::RECIPE_ID), Order::Asc)
            .order_by(Alias::new(recipes::POSITION), Order::Asc)
            .to_owned();
        for row in self.db.query_all_raw(backend.build(&statement)).await? {
            let recipe_id: String = row.try_get("", recipes::RECIPE_ID)?;
            let rule_type: String = row.try_get("", recipes::RULE_TYPE)?;
            let order: String = row.try_get("", recipes::RULE_ORDER)?;
            let rule = ForgeRule {
                rule_type: RuleType::parse(&rule_type)
                    .ok_or_else(|| StoreError::Corrupt(recipe_id.clone()))?,
                order: RuleOrder::parse(&order)
                    .ok_or_else(|| StoreError::Corrupt(recipe_id.clone()))?,
            };
            map.entry(recipe_id).or_default().push(rule);
        }
        Ok(map)
    }

    async fn load_actions(
        &self,
        ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<ActionId>>, StoreError> {
        let mut map: std::collections::HashMap<String, Vec<ActionId>> =
            std::collections::HashMap::new();
        if ids.is_empty() {
            return Ok(map);
        }
        let backend = self.db.get_database_backend();
        let statement = Query::select()
            .columns([recipes::RECIPE_ID, recipes::ACTION_ID])
            .from(Alias::new(recipes::ACTIONS))
            .and_where(Expr::col(Alias::new(recipes::RECIPE_ID)).is_in(ids.iter().cloned()))
            .order_by(Alias::new(recipes::RECIPE_ID), Order::Asc)
            .order_by(Alias::new(recipes::POSITION), Order::Asc)
            .to_owned();
        for row in self.db.query_all_raw(backend.build(&statement)).await? {
            let recipe_id: String = row.try_get("", recipes::RECIPE_ID)?;
            let action_id: String = row.try_get("", recipes::ACTION_ID)?;
            let action = ActionId::parse(&action_id)
                .ok_or_else(|| StoreError::Corrupt(recipe_id.clone()))?;
            map.entry(recipe_id).or_default().push(action);
        }
        Ok(map)
    }

    /// Parse a recipe from a raw JSON value (used by the seed/test tooling).
    pub fn parse_json_recipe(value: &serde_json::Value) -> Option<SavedRecipe> {
        parse_saved_recipe_value(value)
    }

    /// Verify that a stored recipe's actions reproduce its start/target.
    pub fn recipe_is_consistent(recipe: &SavedRecipe) -> bool {
        let path = path_from_actions(recipe.start, &recipe.actions);
        path.first().copied() == Some(recipe.start)
            && path.last().copied() == Some(recipe.target)
            && compact_actions(&recipe.actions) == recipe.compact
    }
}

struct Existing {
    id: String,
    created_at: String,
}

struct BaseRecipe {
    id: String,
    category: String,
    name: String,
    start: i32,
    target: i32,
    compact: String,
    created_at: String,
    updated_at: String,
}

/// WAL journal mode. Resolved through sqlx so we do not need a direct dependency.
fn sqlx_journal_mode_wal() -> sea_orm::sqlx::sqlite::SqliteJournalMode {
    sea_orm::sqlx::sqlite::SqliteJournalMode::Wal
}

// Keep `QueryResult` referenced so future refactors retain the import.
#[allow(dead_code)]
fn _assert_query_result_used(_: &QueryResult) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_saved_recipe_rejects_missing_category() {
        let value = serde_json::json!({
            "id": "recipe-1",
            "name": "Pickaxe head",
            "start": 0,
            "target": 54,
            "rules": [{"type": "punch", "order": "last"}],
            "actions": ["punch"],
            "compact": "1P",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z"
        });
        assert!(parse_saved_recipe_value(&value).is_none());
    }

    #[test]
    fn parse_saved_recipe_accepts_valid_record() {
        let value = serde_json::json!({
            "id": "recipe-1",
            "category": "Tools",
            "name": "Pickaxe head",
            "start": 0,
            "target": 54,
            "rules": [{"type": "punch", "order": "last"}],
            "actions": ["punch"],
            "compact": "1P",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z"
        });
        let parsed = parse_saved_recipe_value(&value).expect("valid");
        assert_eq!(parsed.category, "Tools");
    }

    #[test]
    fn validation_rejects_bad_drafts() {
        use crate::domain::RecipeDraft;
        let draft = RecipeDraft::from_parts(
            "",
            "Pickaxe head",
            0,
            54,
            vec![ForgeRule::new(RuleType::Punch, RuleOrder::Last)],
            vec![ActionId::Punch],
        );
        assert!(draft.validate().is_err());

        let draft = RecipeDraft::from_parts(
            "Tools",
            "Pickaxe head",
            0,
            54,
            vec![
                ForgeRule::new(RuleType::Punch, RuleOrder::Last),
                ForgeRule::new(RuleType::Bend, RuleOrder::Last),
            ],
            vec![ActionId::Punch],
        );
        assert!(draft.validate().is_err());
    }
}
