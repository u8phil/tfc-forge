//! Breadth-first shortest-path solver for the TFC anvil.
//!
//! This is a faithful Rust port of the original TypeScript `src/solver.ts`. The
//! search state is `(work, last three actions)`. BFS explores by depth and
//! iterates actions in canonical [`ACTIONS`] order, so the returned sequence is
//! a true shortest path and is deterministic run to run.

use crate::domain::{
    ACTIONS, ActionId, ForgeRule, ForgeSolution, LIMIT, MIN_WORK, compact_actions,
    has_distinct_rule_order_slots, path_from_actions,
};

/// Number of distinct history codes: three slots, each 0 (none) or 1..8 (action).
const HIST_SIZE: usize = 9 * 9 * 9;

/// Build the history code produced by pushing `action` onto `code`.
fn push_history(code: usize, action_index: usize) -> usize {
    action_index + 1 + 9 * (code % 81)
}

/// Decode a history code into `[last, second_last, third_last]`.
fn history_entries(code: usize) -> [Option<ActionId>; 3] {
    let mut entries = [None, None, None];
    let mut value = code;
    for entry in entries.iter_mut() {
        let digit = value % 9;
        *entry = if digit == 0 {
            None
        } else {
            Some(ACTIONS[digit - 1].id)
        };
        value /= 9;
    }
    entries
}

fn matches(step: Option<ActionId>, rule_type: crate::domain::RuleType) -> bool {
    step.is_some_and(|action| action.rule_type() == rule_type)
}

/// Evaluates a single rule against the last three actions (most recent first).
pub fn rule_satisfied(rule: &ForgeRule, history: &[Option<ActionId>; 3]) -> bool {
    let last = history[0];
    let second_last = history[1];
    let third_last = history[2];
    match rule.order {
        crate::domain::RuleOrder::Any => {
            matches(last, rule.rule_type)
                || matches(second_last, rule.rule_type)
                || matches(third_last, rule.rule_type)
        }
        crate::domain::RuleOrder::NotLast => {
            matches(second_last, rule.rule_type) || matches(third_last, rule.rule_type)
        }
        crate::domain::RuleOrder::Last => matches(last, rule.rule_type),
        crate::domain::RuleOrder::SecondLast => matches(second_last, rule.rule_type),
        crate::domain::RuleOrder::ThirdLast => matches(third_last, rule.rule_type),
    }
}

/// True when every rule holds for the given ordered history.
pub fn rules_satisfied(rules: &[ForgeRule], history: &[Option<ActionId>; 3]) -> bool {
    rules.iter().all(|rule| rule_satisfied(rule, history))
}

/// Breadth-first search for the shortest action sequence that moves `start` to
/// `target` while satisfying all rules, never leaving the inclusive legal range
/// `[MIN_WORK, max_work]`.
///
/// `max_work` defaults to the authoritative [`LIMIT`] (150).
pub fn solve_forge(start: i32, target: i32, rules: &[ForgeRule]) -> Option<ForgeSolution> {
    solve_forge_bounded(start, target, rules, LIMIT)
}

/// As [`solve_forge`], with an explicit inclusive upper work bound.
pub fn solve_forge_bounded(
    start: i32,
    target: i32,
    rules: &[ForgeRule],
    max_work: i32,
) -> Option<ForgeSolution> {
    if !(MIN_WORK..=LIMIT).contains(&max_work) {
        return None;
    }
    if !(MIN_WORK..=max_work).contains(&start) || !(MIN_WORK..=max_work).contains(&target) {
        return None;
    }
    if rules.len() > 3 {
        return None;
    }
    if !has_distinct_rule_order_slots(rules) {
        return None;
    }

    // Empty rule set: no work needed if we are already at the target.
    if rules.is_empty() && start == target {
        return Some(ForgeSolution {
            actions: Vec::new(),
            path: vec![start],
            compact: String::new(),
        });
    }

    let width = (max_work - MIN_WORK + 1) as usize;
    let total_states = width * HIST_SIZE;
    let mut visited = vec![false; total_states];

    // Flat BFS arrays: work value, history code, parent node index, incoming action index.
    let mut work_arr: Vec<i32> = vec![start];
    let mut code_arr: Vec<usize> = vec![0];
    let mut parent_arr: Vec<usize> = vec![usize::MAX];
    let mut action_arr: Vec<usize> = vec![usize::MAX];

    visited[(start - MIN_WORK) as usize * HIST_SIZE] = true;

    let mut head = 0;
    while head < work_arr.len() {
        let work = work_arr[head];
        let code = code_arr[head];

        for (action_index, action) in ACTIONS.iter().enumerate() {
            let next_work = work + action.delta;
            if next_work < MIN_WORK || next_work > max_work {
                continue;
            }

            let next_code = push_history(code, action_index);
            let key = (next_work - MIN_WORK) as usize * HIST_SIZE + next_code;
            if visited[key] {
                continue;
            }
            visited[key] = true;

            let node_index = work_arr.len();
            work_arr.push(next_work);
            code_arr.push(next_code);
            parent_arr.push(head);
            action_arr.push(action_index);

            if next_work == target && rules_satisfied(rules, &history_entries(next_code)) {
                return Some(reconstruct(&parent_arr, &action_arr, node_index, start));
            }
        }
        head += 1;
    }

    None
}

fn reconstruct(
    parent_arr: &[usize],
    action_arr: &[usize],
    mut node_index: usize,
    start: i32,
) -> ForgeSolution {
    let mut actions = Vec::new();
    while parent_arr[node_index] != usize::MAX {
        let action_index = action_arr[node_index];
        actions.push(ACTIONS[action_index].id);
        node_index = parent_arr[node_index];
    }
    actions.reverse();
    let compact = compact_actions(&actions);
    let path = path_from_actions(start, &actions);
    ForgeSolution {
        actions,
        path,
        compact,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{RuleOrder, RuleType};

    fn rule(rule_type: RuleType, order: RuleOrder) -> ForgeRule {
        ForgeRule::new(rule_type, order)
    }

    fn history(code: [Option<ActionId>; 3]) -> [Option<ActionId>; 3] {
        code
    }

    #[test]
    fn generic_hit_matches_all_hit_strengths() {
        assert!(rule_satisfied(
            &rule(RuleType::Hit, RuleOrder::Last),
            &[Some(ActionId::HitLight), None, None]
        ));
        assert!(rule_satisfied(
            &rule(RuleType::Hit, RuleOrder::Last),
            &[Some(ActionId::HitMedium), None, None]
        ));
        assert!(rule_satisfied(
            &rule(RuleType::Hit, RuleOrder::Last),
            &[Some(ActionId::HitHard), None, None]
        ));
        assert!(!rule_satisfied(
            &rule(RuleType::Hit, RuleOrder::Last),
            &[Some(ActionId::Punch), None, None]
        ));
    }

    #[test]
    fn evaluates_each_order_against_last_three() {
        let h = history([
            Some(ActionId::Bend),
            Some(ActionId::Draw),
            Some(ActionId::Punch),
        ]);
        assert!(rule_satisfied(&rule(RuleType::Bend, RuleOrder::Last), &h));
        assert!(rule_satisfied(
            &rule(RuleType::Draw, RuleOrder::SecondLast),
            &h
        ));
        assert!(rule_satisfied(
            &rule(RuleType::Punch, RuleOrder::ThirdLast),
            &h
        ));
        assert!(!rule_satisfied(
            &rule(RuleType::Bend, RuleOrder::NotLast),
            &h
        ));
        assert!(rule_satisfied(
            &rule(RuleType::Draw, RuleOrder::NotLast),
            &h
        ));
        assert!(rule_satisfied(
            &rule(RuleType::Punch, RuleOrder::NotLast),
            &h
        ));
        assert!(rule_satisfied(&rule(RuleType::Punch, RuleOrder::Any), &h));
        assert!(!rule_satisfied(&rule(RuleType::Shrink, RuleOrder::Any), &h));
    }

    fn assert_invariants(start: i32, target: i32, rules: &[ForgeRule]) {
        let solution = solve_forge(start, target, rules).expect("expected a solution");
        assert_eq!(solution.path[0], start);
        assert_eq!(*solution.path.last().unwrap(), target);
        assert_eq!(solution.path.len(), solution.actions.len() + 1);
        for value in &solution.path {
            assert!((MIN_WORK..=LIMIT).contains(value));
        }
        let mut work = start;
        for (i, action) in solution.actions.iter().enumerate() {
            work += action.delta();
            assert_eq!(solution.path[i + 1], work);
        }
        let mut tail = [None, None, None];
        let actions = &solution.actions;
        tail[0] = actions.last().copied();
        tail[1] = actions.iter().rev().nth(1).copied();
        tail[2] = actions.iter().rev().nth(2).copied();
        for rule in rules {
            assert!(rule_satisfied(rule, &tail), "rule {rule:?}");
        }
        assert_eq!(solution.compact, compact_actions(&solution.actions));
    }

    #[test]
    fn solves_fixture_recipes() {
        assert_invariants(
            0,
            109,
            &[
                rule(RuleType::Hit, RuleOrder::Last),
                rule(RuleType::Draw, RuleOrder::SecondLast),
                rule(RuleType::Bend, RuleOrder::ThirdLast),
            ],
        );
        assert_invariants(
            0,
            54,
            &[
                rule(RuleType::Punch, RuleOrder::Last),
                rule(RuleType::Bend, RuleOrder::NotLast),
                rule(RuleType::Draw, RuleOrder::NotLast),
            ],
        );
        assert_invariants(
            0,
            60,
            &[
                rule(RuleType::Punch, RuleOrder::Last),
                rule(RuleType::Hit, RuleOrder::SecondLast),
                rule(RuleType::Upset, RuleOrder::ThirdLast),
            ],
        );
        assert_invariants(
            0,
            100,
            &[
                rule(RuleType::Punch, RuleOrder::Last),
                rule(RuleType::Bend, RuleOrder::NotLast),
                rule(RuleType::Draw, RuleOrder::NotLast),
            ],
        );
    }

    #[test]
    fn never_goes_negative_from_zero_start() {
        let solution =
            solve_forge(0, 0, &[rule(RuleType::Hit, RuleOrder::Last)]).expect("solution");
        assert!(!solution.actions.is_empty());
        for value in &solution.path {
            assert!(*value >= MIN_WORK && *value <= LIMIT);
        }
    }

    #[test]
    fn finds_non_empty_return_path_when_start_equals_target() {
        let solution =
            solve_forge(50, 50, &[rule(RuleType::Shrink, RuleOrder::Last)]).expect("solution");
        assert!(!solution.actions.is_empty());
        assert_eq!(solution.path[0], 50);
        assert_eq!(*solution.path.last().unwrap(), 50);
    }

    #[test]
    fn empty_solution_for_start_equals_target_with_no_rules() {
        let solution = solve_forge(42, 42, &[]).expect("solution");
        assert_eq!(solution.actions, Vec::<ActionId>::new());
        assert_eq!(solution.path, vec![42]);
        assert_eq!(solution.compact, "");
    }

    #[test]
    fn is_deterministic() {
        let rules = [
            rule(RuleType::Hit, RuleOrder::Last),
            rule(RuleType::Draw, RuleOrder::SecondLast),
            rule(RuleType::Bend, RuleOrder::ThirdLast),
        ];
        assert_eq!(solve_forge(0, 109, &rules), solve_forge(0, 109, &rules));
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(solve_forge(-1, 10, &[]).is_none());
        assert!(solve_forge(0, 151, &[]).is_none());
        assert!(solve_forge_bounded(0, 10, &[], 999).is_none());
        assert!(solve_forge_bounded(0, 1, &[], 1).is_none());
        assert!(solve_forge_bounded(0, 0, &[rule(RuleType::Punch, RuleOrder::Last)], 0).is_none());
    }

    #[test]
    fn rejects_duplicate_and_impossible_rule_sets() {
        assert!(
            solve_forge(
                0,
                54,
                &[
                    rule(RuleType::Punch, RuleOrder::Last),
                    rule(RuleType::Bend, RuleOrder::Last),
                ]
            )
            .is_none()
        );
        assert!(
            solve_forge(
                0,
                54,
                &[
                    rule(RuleType::Punch, RuleOrder::SecondLast),
                    rule(RuleType::Bend, RuleOrder::SecondLast),
                ]
            )
            .is_none()
        );
        assert!(
            solve_forge(
                0,
                54,
                &[
                    rule(RuleType::Punch, RuleOrder::ThirdLast),
                    rule(RuleType::Bend, RuleOrder::ThirdLast),
                ]
            )
            .is_none()
        );
        assert!(
            solve_forge(
                0,
                54,
                &[
                    rule(RuleType::Punch, RuleOrder::NotLast),
                    rule(RuleType::Bend, RuleOrder::NotLast),
                    rule(RuleType::Draw, RuleOrder::NotLast),
                ]
            )
            .is_none()
        );
        assert!(
            solve_forge(
                0,
                54,
                &[
                    rule(RuleType::Punch, RuleOrder::SecondLast),
                    rule(RuleType::Bend, RuleOrder::ThirdLast),
                    rule(RuleType::Draw, RuleOrder::NotLast),
                ]
            )
            .is_none()
        );
    }

    #[test]
    fn keeps_punch_last_alongside_punch_any() {
        let solution = solve_forge(
            0,
            54,
            &[
                rule(RuleType::Punch, RuleOrder::Last),
                rule(RuleType::Punch, RuleOrder::Any),
            ],
        );
        assert!(solution.is_some());
    }
}
