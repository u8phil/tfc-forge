//! Behavior tests for the SQLite recipe store's application CRUD and
//! validation invariants. Complements the focused end-to-end test in
//! `persistence.rs`.

use tempfile::TempDir;
use tfc_forge::{ActionId, ForgeRule, RecipeDraft, RecipeStore, RuleOrder, RuleType};

fn rule(rule_type: RuleType, order: RuleOrder) -> ForgeRule {
    ForgeRule::new(rule_type, order)
}

async fn store() -> (TempDir, RecipeStore) {
    let dir = TempDir::new().expect("temp dir");
    let store = RecipeStore::open(dir.path().join("recipes.sqlite3"))
        .await
        .expect("open");
    (dir, store)
}

fn valid_draft(category: &str, name: &str) -> RecipeDraft {
    RecipeDraft::from_parts(
        category,
        name,
        0,
        14,
        vec![rule(RuleType::Bend, RuleOrder::Last)],
        vec![ActionId::Bend, ActionId::Bend],
    )
}

#[tokio::test]
async fn list_is_stable_by_category_then_name() {
    let (_dir, store) = store().await;
    store.save(valid_draft("Zeta", "A")).await.unwrap();
    store.save(valid_draft("Alpha", "Z")).await.unwrap();
    store.save(valid_draft("Alpha", "A")).await.unwrap();

    let listed = store.list().await.unwrap();
    // Only our three additions are in these categories.
    let ours: Vec<(&str, &str)> = listed
        .iter()
        .filter(|r| matches!(r.category.as_str(), "Alpha" | "Zeta"))
        .map(|r| (r.category.as_str(), r.name.as_str()))
        .collect();
    assert_eq!(ours, vec![("Alpha", "A"), ("Alpha", "Z"), ("Zeta", "A")]);
}

#[tokio::test]
async fn stored_rules_and_actions_keep_order() {
    let (_dir, store) = store().await;
    // 3 bends = 21, 2 shrinks = 53, punch = 55.
    let draft = RecipeDraft::from_parts(
        "Ordering",
        "Sequence",
        0,
        55,
        vec![
            rule(RuleType::Punch, RuleOrder::Last),
            rule(RuleType::Shrink, RuleOrder::NotLast),
        ],
        vec![
            ActionId::Bend,
            ActionId::Bend,
            ActionId::Bend,
            ActionId::Shrink,
            ActionId::Shrink,
            ActionId::Punch,
        ],
    );
    let saved = store.save(draft).await.unwrap();
    let listed = store.list().await.unwrap();
    let loaded = listed.iter().find(|r| r.id == saved.id).unwrap();
    assert_eq!(loaded.rules, saved.rules);
    assert_eq!(loaded.actions, saved.actions);
    assert_eq!(loaded.compact, saved.compact);
}

#[tokio::test]
async fn zero_action_start_equals_target_round_trips() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("recipes.sqlite3");

    // start == target is legal with no actions; validation still requires one
    // valid rule. Reloading must yield an empty action list, not corruption.
    let draft = RecipeDraft::from_parts(
        "Stationary",
        "No Op",
        37,
        37,
        vec![rule(RuleType::Bend, RuleOrder::Last)],
        Vec::new(),
    );

    let id = {
        let store = RecipeStore::open(&db_path).await.unwrap();
        let saved = store.save(draft).await.unwrap();
        assert!(saved.actions.is_empty());
        saved.id
    };

    let store = RecipeStore::open(&db_path).await.unwrap();
    let listed = store.list().await.unwrap();
    let loaded = listed
        .iter()
        .find(|recipe| recipe.id == id)
        .expect("reloaded recipe");
    assert_eq!(loaded.actions, Vec::<ActionId>::new());
    assert_eq!(loaded.compact, "");
    assert_eq!(loaded.rules.len(), 1);
    assert!(RecipeStore::recipe_is_consistent(loaded));
}

#[tokio::test]
async fn save_rejects_bad_drafts() {
    let (_dir, store) = store().await;

    // Empty category.
    let mut draft = valid_draft("Tools", "X");
    draft.category = "   ".to_string();
    assert!(store.save(draft).await.is_err());

    // Over-long category.
    let mut draft = valid_draft("Tools", "X");
    draft.category = "c".repeat(101);
    assert!(store.save(draft).await.is_err());

    // Over-long name.
    let mut draft = valid_draft("Tools", "X");
    draft.name = "n".repeat(201);
    assert!(store.save(draft).await.is_err());

    // Out-of-range target.
    let mut draft = valid_draft("Tools", "X");
    draft.target = 151;
    assert!(store.save(draft).await.is_err());

    // No rules.
    let mut draft = valid_draft("Tools", "X");
    draft.rules.clear();
    assert!(store.save(draft).await.is_err());

    // Duplicate pinned order slots.
    let mut draft = valid_draft("Tools", "X");
    draft.rules = vec![
        rule(RuleType::Punch, RuleOrder::Last),
        rule(RuleType::Bend, RuleOrder::Last),
    ];
    assert!(store.save(draft).await.is_err());

    // Actions that do not reach the target.
    let mut draft = valid_draft("Tools", "X");
    draft.actions = vec![ActionId::Bend];
    assert!(store.save(draft).await.is_err());

    // Actions that leave the legal range.
    let mut draft = valid_draft("Tools", "X");
    draft.start = 0;
    draft.target = -15;
    draft.actions = vec![ActionId::Draw];
    assert!(store.save(draft).await.is_err());

    let mut draft = valid_draft("Tools", "X");
    draft.start = 140;
    draft.target = 151;
    draft.actions = vec![ActionId::Bend];
    assert!(store.save(draft).await.is_err());
}

#[tokio::test]
async fn upsert_is_case_insensitive_within_category() {
    let (_dir, store) = store().await;
    let first = store.save(valid_draft("Tools", "Pickaxe")).await.unwrap();
    let second = store.save(valid_draft("tools", "pickaxe")).await.unwrap();
    assert_eq!(first.id, second.id, "case-insensitive key must upsert");
    assert_eq!(second.category, "tools");
    assert_eq!(second.name, "pickaxe");
    assert_eq!(second.created_at, first.created_at);

    let listed = store.list().await.unwrap();
    let matches: Vec<_> = listed
        .iter()
        .filter(|r| r.category == "tools" && r.name.eq_ignore_ascii_case("pickaxe"))
        .collect();
    assert_eq!(matches.len(), 1);
}

#[tokio::test]
async fn same_name_in_different_categories_coexist() {
    let (_dir, store) = store().await;
    let a = store
        .save(valid_draft("Wrought Iron", "Rod"))
        .await
        .unwrap();
    let b = store.save(valid_draft("Bronze", "Rod")).await.unwrap();
    assert_ne!(a.id, b.id);
    let listed = store.list().await.unwrap();
    let rods = listed.iter().filter(|r| r.name == "Rod").count();
    assert_eq!(rods, 2, "only the two user-created Rods exist");
}

#[tokio::test]
async fn persistence_survives_reopen_and_path_is_exposed() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("nested").join("recipes.sqlite3");
    {
        let store = RecipeStore::open(&db_path).await.unwrap();
        assert_eq!(store.path(), db_path.as_path());
        store.save(valid_draft("Persisted", "One")).await.unwrap();
        assert_eq!(store.list().await.unwrap().len(), 1);
    }
    let store = RecipeStore::open(&db_path).await.unwrap();
    let listed = store.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed.iter().any(|r| r.name == "One"));
}

#[tokio::test]
async fn open_default_resolves_under_config_dir() {
    // `open_default` must resolve a directory and open successfully. It writes
    // outside the project directory (the OS config dir) by design. A fresh
    // store is empty, and an upgraded one is empty of bundled recipes.
    let store = RecipeStore::open_default().await.expect("open default");
    assert!(store.path().ends_with("tfc-forge/recipes.sqlite3"));
    store.list().await.expect("list default");
}
