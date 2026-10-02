//! Focused persistence test for the SQLite recipe store.
//!
//! Exercises the exact end-to-end flow required for stage 1:
//!   1. open a fresh temp DB and confirm it starts empty (no bundled recipes)
//!   2. save the same recipe name in two different categories
//!   3. reopen the DB (migrations idempotent) and confirm state
//!   4. delete the saved recipes
//!   5. reopen again and prove nothing reappears (no reseeding)

use tempfile::TempDir;
use tfc_forge::{ActionId, ForgeRule, RecipeDraft, RecipeStore, RuleOrder, RuleType};

fn draft(category: &str, name: &str, target: i32) -> RecipeDraft {
    // A short, valid action path: two bends reach 14 from 0.
    let actions = vec![ActionId::Bend, ActionId::Bend];
    let rules = vec![ForgeRule::new(RuleType::Bend, RuleOrder::Last)];
    RecipeDraft::from_parts(category, name, 0, target, rules, actions)
}

#[tokio::test]
async fn persistence_round_trip_and_no_reseed() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("recipes.sqlite3");

    // 1. Fresh open: migrations run, but no default recipes are bundled.
    let store = RecipeStore::open(&db_path).await.expect("open");
    assert!(
        store.list().await.expect("list fresh").is_empty(),
        "a fresh database must start with zero recipes"
    );

    // 2. Same name in two different categories is explicitly allowed.
    let wrought = draft("Wrought Iron", "Integration Rod", 14);
    let bronze = draft("Bronze", "Integration Rod", 14);
    let saved_wrought = store.save(wrought.clone()).await.expect("save wrought");
    let saved_bronze = store.save(bronze.clone()).await.expect("save bronze");
    assert_ne!(
        saved_wrought.id, saved_bronze.id,
        "same name in different categories must be distinct rows"
    );

    let after_save = store.list().await.expect("list after save");
    assert_eq!(after_save.len(), 2);
    let matching = after_save
        .iter()
        .filter(|recipe| recipe.name == "Integration Rod")
        .count();
    assert_eq!(matching, 2);

    // Upsert the same (category, name) again preserves created_at and changes updated_at.
    let resaved = store.save(wrought).await.expect("resave");
    assert_eq!(resaved.id, saved_wrought.id, "upsert must reuse the row");
    assert_eq!(
        resaved.created_at, saved_wrought.created_at,
        "created_at preserved on upsert"
    );
    assert!(
        resaved.updated_at >= saved_wrought.updated_at,
        "updated_at refreshed on upsert"
    );

    // 3. Reopen: migrations are idempotent, no recipes are added.
    drop(store);
    let store = RecipeStore::open(&db_path).await.expect("reopen");
    let after_reopen = store.list().await.expect("list after reopen");
    assert_eq!(after_reopen.len(), 2, "reopening must not add recipes");

    // 4. Delete the two added recipes.
    assert!(
        store
            .delete(&saved_wrought.id)
            .await
            .expect("delete wrought")
    );
    assert!(store.delete(&saved_bronze.id).await.expect("delete bronze"));
    // Deleting a missing id is a no-op that reports false.
    assert!(!store.delete(&saved_wrought.id).await.expect("delete again"));
    let after_delete = store.list().await.expect("list after delete");
    assert!(after_delete.is_empty());

    // 5. Reopen after emptying: still empty, because nothing is seeded.
    drop(store);
    let store = RecipeStore::open(&db_path)
        .await
        .expect("reopen after empty");
    let final_state = store.list().await.expect("final list");
    assert!(
        final_state.is_empty(),
        "recipes must not reappear after restart (no reseeding)"
    );
}
