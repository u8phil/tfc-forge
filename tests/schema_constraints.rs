use sea_orm::{ConnectionTrait, DbBackend, Statement};
use tfc_forge::{ActionId, ForgeRule, RecipeDraft, RecipeStore, RuleOrder, RuleType};

#[tokio::test]
async fn foreign_keys_enforced_and_case_insensitive_unique() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = RecipeStore::open(dir.path().join("s.sqlite3"))
        .await
        .unwrap();
    let db = store.connection();
    // Per-connection foreign_keys pragma must be ON.
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys;",
        ))
        .await
        .unwrap()
        .unwrap();
    let fk: i32 = row.try_get("", "foreign_keys").unwrap();
    assert_eq!(fk, 1, "foreign_keys pragma must be enabled");

    // Raw insert bypassing app logic with a bogus FK must fail.
    let bad = db.execute_raw(Statement::from_string(
        DbBackend::Sqlite,
        "INSERT INTO recipe_rules (recipe_id, position, rule_type, rule_order) VALUES ('nope',0,'hit','last')",
    )).await;
    assert!(bad.is_err(), "FK violation must be rejected");

    // Case-insensitive uniqueness is enforced at the schema level.
    let draft = RecipeDraft::from_parts(
        "Case",
        "Key",
        0,
        14,
        vec![ForgeRule::new(RuleType::Bend, RuleOrder::Last)],
        vec![ActionId::Bend, ActionId::Bend],
    );
    store.save(draft).await.unwrap();
    let raw = db.execute_raw(Statement::from_string(
        DbBackend::Sqlite,
        "INSERT INTO recipes (id,category,category_key,name,name_key,start,target,compact,created_at,updated_at) VALUES ('x','CASE','case','KEY','key',0,14,'2B','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')",
    )).await;
    assert!(
        raw.is_err(),
        "case-insensitive unique must reject duplicate key"
    );
}
