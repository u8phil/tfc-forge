//! Native egui views for the TFC Forge Planner.
//!
//! The panels are split by concern and read/write state owned by
//! [`crate::app::ForgeApp`]:
//!   - [`anvil`]: rule slots, the 2x4 action grid, the 0..150 work track
//!   - [`solution`]: sequence, metrics, path strip, step table, save section
//!   - [`library`]: category filter, fuzzy search, select/delete
//!   - [`recognizer`]: image chooser/drop/paste, preview, apply
//!
//! This module also holds the small non-visual helpers shared by the views;
//! those are unit tested by the inline `tests` module at the bottom of this file.

pub mod anvil;
pub mod library;
pub mod recognizer;
pub mod solution;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use tfc_forge::SavedRecipe;

/// The stable `Category / Recipe name` label used everywhere a recipe is
/// displayed, so `Rods / Rod` and `Steel / Rod` are never ambiguous.
pub fn recipe_label(category: &str, name: &str) -> String {
    format!("{} / {}", category.trim(), name.trim())
}

/// Display label for a saved recipe.
pub fn saved_label(recipe: &SavedRecipe) -> String {
    recipe_label(&recipe.category, &recipe.name)
}

/// Distinct, sorted category names from a recipe list, ignoring blanks.
pub fn category_options(recipes: &[SavedRecipe]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for recipe in recipes {
        let category = recipe.category.trim();
        if category.is_empty() {
            continue;
        }
        if !seen.iter().any(|existing| existing == category) {
            seen.push(category.to_string());
        }
    }
    seen.sort_by_key(|category| category.to_lowercase());
    seen
}

/// Filter saved recipes by an exact category (empty means all).
pub fn filter_by_category(recipes: &[SavedRecipe], category: &str) -> Vec<SavedRecipe> {
    if category.is_empty() {
        recipes.to_vec()
    } else {
        recipes
            .iter()
            .filter(|recipe| recipe.category == category)
            .cloned()
            .collect()
    }
}

/// Fuzzy-search a recipe list over the combined `Category / Name` display,
/// returning results in the original (stable) order when the query is empty.
///
/// When the query is non-empty, results are ordered by descending fuzzy score
/// with a stable tiebreak on the original position.
pub fn fuzzy_filter(recipes: &[SavedRecipe], query: &str) -> Vec<SavedRecipe> {
    let query = query.trim();
    if query.is_empty() {
        return recipes.to_vec();
    }
    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(i64, usize, SavedRecipe)> = recipes
        .iter()
        .enumerate()
        .filter_map(|(index, recipe)| {
            let label = saved_label(recipe);
            let score = matcher
                .fuzzy_match(&label, query)
                .or_else(|| matcher.fuzzy_match(&recipe.compact, query))?;
            Some((score, index, recipe.clone()))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, recipe)| recipe).collect()
}

/// Clamp a work value into the authoritative inclusive `0..=150` range.
pub fn clamp_work(value: i32) -> i32 {
    value.clamp(tfc_forge::MIN_WORK, tfc_forge::LIMIT)
}

/// Cycle `current` to the next entry in `list`, wrapping around.
pub fn cycle<T: Copy + PartialEq>(list: &[T], current: T) -> T {
    match list.iter().position(|candidate| *candidate == current) {
        Some(index) => list[(index + 1) % list.len()],
        None => list[0],
    }
}

/// Advance through `list` starting after `current` and return the first
/// candidate for which `accept(candidate)` holds, or `current` when none does.
pub fn cycle_until<T: Copy + PartialEq>(
    list: &[T],
    current: T,
    mut accept: impl FnMut(T) -> bool,
) -> T {
    let start = list
        .iter()
        .position(|candidate| *candidate == current)
        .unwrap_or(0);
    for step in 1..=list.len() {
        let candidate = list[(start + step) % list.len()];
        if accept(candidate) {
            return candidate;
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe(category: &str, name: &str, compact: &str) -> SavedRecipe {
        SavedRecipe {
            id: format!("{category}-{name}"),
            category: category.to_string(),
            name: name.to_string(),
            start: 0,
            target: 10,
            rules: Vec::new(),
            actions: Vec::new(),
            compact: compact.to_string(),
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        }
    }

    #[test]
    fn labels_always_pair_category_and_name() {
        assert_eq!(recipe_label(" Rods ", " Rod "), "Rods / Rod");
        assert_eq!(saved_label(&recipe("Steel", "Rod", "")), "Steel / Rod");
    }

    #[test]
    fn category_options_are_distinct_and_sorted() {
        let recipes = vec![
            recipe("Steel", "A", ""),
            recipe("iron", "B", ""),
            recipe("Steel", "C", ""),
        ];
        let options = category_options(&recipes);
        assert_eq!(options, vec!["iron".to_string(), "Steel".to_string()]);
    }

    #[test]
    fn category_filter_is_exact_and_empty_means_all() {
        let recipes = vec![recipe("Steel", "A", ""), recipe("Iron", "B", "")];
        assert_eq!(filter_by_category(&recipes, "").len(), 2);
        assert_eq!(filter_by_category(&recipes, "Steel").len(), 1);
        assert_eq!(filter_by_category(&recipes, "steel").len(), 0);
    }

    #[test]
    fn fuzzy_search_matches_combined_label_and_keeps_stable_empty_order() {
        let recipes = vec![
            recipe("Rods", "Rod", "1P"),
            recipe("Steel", "Rod", "1B"),
            recipe("Iron", "Plate", "1D"),
        ];
        // Empty query preserves the input order exactly.
        let all = fuzzy_filter(&recipes, "");
        assert_eq!(all[0].category, "Rods");
        assert_eq!(all[2].category, "Iron");

        // The combined "Category / Name" is searchable.
        let hits = fuzzy_filter(&recipes, "Steel / Rod");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].category, "Steel");

        // Compact notation is searchable too.
        let compact_hits = fuzzy_filter(&recipes, "1D");
        assert_eq!(compact_hits.len(), 1);
        assert_eq!(compact_hits[0].name, "Plate");
    }

    #[test]
    fn clamp_work_is_inclusive() {
        assert_eq!(clamp_work(-5), 0);
        assert_eq!(clamp_work(0), 0);
        assert_eq!(clamp_work(150), 150);
        assert_eq!(clamp_work(999), 150);
    }

    #[test]
    fn cycle_wraps_and_cycle_until_skips_rejected() {
        let list = [1, 2, 3];
        assert_eq!(cycle(&list, 1), 2);
        assert_eq!(cycle(&list, 3), 1);

        // Skip anything below 3, starting after 1: 2 rejected, 3 accepted.
        let picked = cycle_until(&list, 1, |candidate| candidate >= 3);
        assert_eq!(picked, 3);
        // No candidate accepted -> stays put.
        assert_eq!(cycle_until(&list, 2, |_| false), 2);
    }
}
