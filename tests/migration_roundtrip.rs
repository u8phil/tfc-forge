//! Migration behavior tests.
//!
//! Covers two things:
//!   - rollback/re-apply of the full migrator leaves a fresh, empty store
//!   - upgrading a database that still holds the old seeded `default:*`
//!     recipes purges only those rows, leaving user recipes intact

use std::path::Path;

use sea_orm::sea_query::{Alias, Query};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use tempfile::TempDir;
use tfc_forge::RecipeStore;
use tfc_forge_migration::{Migrator, MigratorTrait};

/// Open a connection with SQLite foreign-key enforcement enabled, matching the
/// pragma the application sets on every connection. The cascade used by the
/// removal migration depends on it.
async fn connect_with_foreign_keys(path: &Path) -> DatabaseConnection {
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let mut options = ConnectOptions::new(url);
    options.map_sqlx_sqlite_opts(|sqlite| sqlite.foreign_keys(true));
    Database::connect(options).await.expect("connect")
}

async fn insert_recipe(conn: &DatabaseConnection, id: &str, category: &str, name: &str) {
    let backend = conn.get_database_backend();
    let stmt = Query::insert()
        .into_table(Alias::new("recipes"))
        .columns([
            Alias::new("id"),
            Alias::new("category"),
            Alias::new("category_key"),
            Alias::new("name"),
            Alias::new("name_key"),
            Alias::new("start"),
            Alias::new("target"),
            Alias::new("compact"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            id.into(),
            category.into(),
            category.to_lowercase().into(),
            name.into(),
            name.to_lowercase().into(),
            0.into(),
            14.into(),
            "2B".into(),
            "2026-01-01T00:00:00.000Z".into(),
            "2026-01-01T00:00:00.000Z".into(),
        ])
        .to_owned();
    conn.execute_raw(backend.build(&stmt)).await.unwrap();
}

async fn insert_rule(conn: &DatabaseConnection, recipe_id: &str, position: i32) {
    let backend = conn.get_database_backend();
    let stmt = Query::insert()
        .into_table(Alias::new("recipe_rules"))
        .columns([
            Alias::new("recipe_id"),
            Alias::new("position"),
            Alias::new("rule_type"),
            Alias::new("rule_order"),
        ])
        .values_panic([
            recipe_id.into(),
            position.into(),
            "bend".into(),
            "last".into(),
        ])
        .to_owned();
    conn.execute_raw(backend.build(&stmt)).await.unwrap();
}

async fn insert_action(conn: &DatabaseConnection, recipe_id: &str, position: i32) {
    let backend = conn.get_database_backend();
    let stmt = Query::insert()
        .into_table(Alias::new("recipe_actions"))
        .columns([
            Alias::new("recipe_id"),
            Alias::new("position"),
            Alias::new("action_id"),
        ])
        .values_panic([recipe_id.into(), position.into(), "bend".into()])
        .to_owned();
    conn.execute_raw(backend.build(&stmt)).await.unwrap();
}

async fn count(conn: &DatabaseConnection, sql: &str) -> i64 {
    let row = conn
        .query_one_raw(Statement::from_string(DbBackend::Sqlite, sql.to_owned()))
        .await
        .unwrap()
        .unwrap();
    row.try_get("", "c").unwrap()
}

#[tokio::test]
async fn migrations_down_and_up_are_reversible() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("recipes.sqlite3");
    let store = RecipeStore::open(&path).await.unwrap();
    // No bundled recipes: a fresh database is empty.
    assert_eq!(store.list().await.unwrap().len(), 0);
    let conn = store.connection().clone();
    drop(store);
    // Roll everything back.
    Migrator::down(&conn, None).await.unwrap();
    // Re-apply.
    Migrator::up(&conn, None).await.unwrap();
    let store = RecipeStore::open(&path).await.unwrap();
    assert_eq!(store.list().await.unwrap().len(), 0);
}

#[tokio::test]
async fn upgrading_purges_only_default_recipes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("recipes.sqlite3");
    let conn = connect_with_foreign_keys(&path).await;

    // Apply only the schema migration and the retained history placeholder,
    // simulating a database written by the old build that had seeded defaults.
    Migrator::up(&conn, Some(2)).await.unwrap();

    let default_id = "default:tools::upgrade-test";
    let user_id = "3f2a1b90-1111-4a2b-9f3c-0123456789ab";

    // A former default recipe with child rules and actions.
    insert_recipe(&conn, default_id, "Tools", "Upgrade Test").await;
    insert_rule(&conn, default_id, 0).await;
    insert_action(&conn, default_id, 0).await;
    insert_action(&conn, default_id, 1).await;

    // A user-created recipe (UUID id) with its own children.
    insert_recipe(&conn, user_id, "User", "Kept").await;
    insert_rule(&conn, user_id, 0).await;
    insert_action(&conn, user_id, 0).await;
    insert_action(&conn, user_id, 1).await;

    assert_eq!(count(&conn, "SELECT COUNT(*) AS c FROM recipes").await, 2);
    assert_eq!(
        count(&conn, "SELECT COUNT(*) AS c FROM recipe_rules").await,
        2
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) AS c FROM recipe_actions").await,
        4
    );

    // Apply the remaining migrations: the purge runs.
    Migrator::up(&conn, None).await.unwrap();

    // The default recipe and its cascade-deleted children are gone.
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) AS c FROM recipes WHERE id LIKE 'default:%'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) AS c FROM recipe_rules WHERE recipe_id LIKE 'default:%'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) AS c FROM recipe_actions WHERE recipe_id LIKE 'default:%'"
        )
        .await,
        0
    );

    // The user recipe and its children survive untouched.
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) AS c FROM recipes WHERE id NOT LIKE 'default:%'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) AS c FROM recipe_rules \
             WHERE recipe_id = '3f2a1b90-1111-4a2b-9f3c-0123456789ab'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) AS c FROM recipe_actions \
             WHERE recipe_id = '3f2a1b90-1111-4a2b-9f3c-0123456789ab'"
        )
        .await,
        2
    );

    // The application sees exactly the surviving user recipe after reopening.
    let store = RecipeStore::open(&path).await.unwrap();
    let listed = store.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, user_id);
    assert_eq!(listed[0].name, "Kept");
    assert!(RecipeStore::recipe_is_consistent(&listed[0]));
}
