//! Integration tests for the domain model and solver.
//!
//! Ported from the original TypeScript `tests/domain.test.ts` and
//! `tests/solver.test.ts` where behavior overlaps, and extended with an
//! independent naive BFS that double-checks shortest-path lengths.

use tfc_forge::{
    ACTIONS, ActionId, ForgeRule, LIMIT, RecipeDraft, RuleOrder, RuleType, compact_actions,
    has_distinct_rule_order_slots, rule_satisfied, solve_forge, solve_forge_bounded,
};

fn rule(rule_type: RuleType, order: RuleOrder) -> ForgeRule {
    ForgeRule::new(rule_type, order)
}

#[test]
fn action_table_is_exact() {
    assert_eq!(ACTIONS.len(), 8);
    assert_eq!(LIMIT, 150);
    let expected = [
        (ActionId::HitLight, -3, "LH"),
        (ActionId::HitMedium, -6, "MD"),
        (ActionId::HitHard, -9, "HH"),
        (ActionId::Draw, -15, "D"),
        (ActionId::Punch, 2, "P"),
        (ActionId::Bend, 7, "B"),
        (ActionId::Upset, 13, "U"),
        (ActionId::Shrink, 16, "S"),
    ];
    for (index, (id, delta, symbol)) in expected.iter().enumerate() {
        assert_eq!(ACTIONS[index].id, *id);
        assert_eq!(ACTIONS[index].delta, *delta);
        assert_eq!(ACTIONS[index].symbol, *symbol);
    }
}

#[test]
fn rule_slot_assignment_matches_reference_tests() {
    // Rejections.
    assert!(!has_distinct_rule_order_slots(&[
        rule(RuleType::Hit, RuleOrder::Last),
        rule(RuleType::Draw, RuleOrder::Last),
    ]));
    assert!(!has_distinct_rule_order_slots(&[
        rule(RuleType::Hit, RuleOrder::SecondLast),
        rule(RuleType::Draw, RuleOrder::SecondLast),
    ]));
    assert!(!has_distinct_rule_order_slots(&[
        rule(RuleType::Hit, RuleOrder::ThirdLast),
        rule(RuleType::Draw, RuleOrder::ThirdLast),
    ]));
    assert!(!has_distinct_rule_order_slots(&[
        rule(RuleType::Hit, RuleOrder::NotLast),
        rule(RuleType::Bend, RuleOrder::NotLast),
        rule(RuleType::Draw, RuleOrder::NotLast),
    ]));
    assert!(!has_distinct_rule_order_slots(&[
        rule(RuleType::Hit, RuleOrder::SecondLast),
        rule(RuleType::Bend, RuleOrder::ThirdLast),
        rule(RuleType::Draw, RuleOrder::NotLast),
    ]));

    // Acceptances.
    assert!(has_distinct_rule_order_slots(&[
        rule(RuleType::Hit, RuleOrder::Any),
        rule(RuleType::Draw, RuleOrder::Any),
        rule(RuleType::Bend, RuleOrder::Any),
    ]));
    assert!(has_distinct_rule_order_slots(&[
        rule(RuleType::Hit, RuleOrder::NotLast),
        rule(RuleType::Draw, RuleOrder::NotLast),
    ]));
    assert!(has_distinct_rule_order_slots(&[
        rule(RuleType::Punch, RuleOrder::Last),
        rule(RuleType::Punch, RuleOrder::Any),
    ]));
    assert!(has_distinct_rule_order_slots(&[
        rule(RuleType::Hit, RuleOrder::Last),
        rule(RuleType::Draw, RuleOrder::SecondLast),
        rule(RuleType::Bend, RuleOrder::ThirdLast),
    ]));
    assert!(has_distinct_rule_order_slots(&[]));
    assert!(has_distinct_rule_order_slots(&[rule(
        RuleType::Hit,
        RuleOrder::NotLast
    )]));
}

#[test]
fn compact_notation_examples() {
    assert_eq!(compact_actions(&[]), "");
    assert_eq!(
        compact_actions(&[ActionId::HitLight, ActionId::HitMedium, ActionId::HitHard,]),
        "1LH 1MD 1HH"
    );
    assert_eq!(
        compact_actions(&[
            ActionId::Upset,
            ActionId::Shrink,
            ActionId::Shrink,
            ActionId::Shrink,
            ActionId::Upset,
            ActionId::Shrink,
            ActionId::Shrink,
            ActionId::Shrink,
            ActionId::Bend,
        ]),
        "1U 3S 1U 3S 1B"
    );
}

/// Independent, deliberately naive BFS used to double-check shortest length.
fn reference_shortest_length(start: i32, target: i32, rules: &[ForgeRule]) -> i32 {
    use std::collections::HashSet;
    use std::collections::VecDeque;

    #[derive(Clone)]
    struct Node {
        work: i32,
        history: [Option<ActionId>; 3],
        depth: i32,
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<Node> = VecDeque::new();
    let initial = Node {
        work: start,
        history: [None, None, None],
        depth: 0,
    };
    seen.insert(format!("{start}|null,null,null"));
    queue.push_back(initial);

    while let Some(node) = queue.pop_front() {
        if node.depth > 0
            && node.work == target
            && rules.iter().all(|r| rule_satisfied(r, &node.history))
        {
            return node.depth;
        }
        for action in ACTIONS {
            let next_work = node.work + action.delta;
            if !(0..=LIMIT).contains(&next_work) {
                continue;
            }
            let next_history = [Some(action.id), node.history[0], node.history[1]];
            let key = format!(
                "{next_work}|{}, {}, {}",
                next_history[0].map(|a| a.as_str()).unwrap_or("null"),
                next_history[1].map(|a| a.as_str()).unwrap_or("null"),
                next_history[2].map(|a| a.as_str()).unwrap_or("null"),
            );
            if !seen.insert(key) {
                continue;
            }
            queue.push_back(Node {
                work: next_work,
                history: next_history,
                depth: node.depth + 1,
            });
        }
    }
    -1
}

#[test]
fn solutions_are_truly_shortest() {
    let cases: Vec<(i32, i32, Vec<ForgeRule>)> = vec![
        (
            0,
            109,
            vec![
                rule(RuleType::Hit, RuleOrder::Last),
                rule(RuleType::Draw, RuleOrder::SecondLast),
                rule(RuleType::Bend, RuleOrder::ThirdLast),
            ],
        ),
        (0, 54, vec![rule(RuleType::Punch, RuleOrder::Last)]),
        (
            37,
            84,
            vec![
                rule(RuleType::Bend, RuleOrder::Any),
                rule(RuleType::Draw, RuleOrder::NotLast),
            ],
        ),
        (
            120,
            30,
            vec![
                rule(RuleType::Hit, RuleOrder::ThirdLast),
                rule(RuleType::Shrink, RuleOrder::Last),
            ],
        ),
        (150, 0, vec![rule(RuleType::Draw, RuleOrder::Any)]),
    ];
    for (start, target, rules) in cases {
        let solution = solve_forge(start, target, &rules).expect("solution");
        let reference = reference_shortest_length(start, target, &rules);
        assert_eq!(
            solution.actions.len() as i32,
            reference,
            "case {start}->{target}"
        );
    }
}

#[test]
fn honours_reduced_max_work() {
    assert!(solve_forge_bounded(0, 1, &[], 1).is_none());
    assert!(solve_forge_bounded(0, 0, &[rule(RuleType::Punch, RuleOrder::Last)], 0).is_none());
    let bounded = solve_forge_bounded(0, 10, &[], LIMIT).expect("solution");
    assert!(!bounded.actions.is_empty());
    for value in &bounded.path {
        assert!(*value <= 10 + LIMIT);
    }
}

#[test]
fn draft_validation_matches_server_invariants() {
    let valid = RecipeDraft::from_parts(
        "  Wrought Iron  ",
        "  Tongue Part ",
        0,
        109,
        vec![
            rule(RuleType::Hit, RuleOrder::Last),
            rule(RuleType::Draw, RuleOrder::SecondLast),
            rule(RuleType::Bend, RuleOrder::ThirdLast),
        ],
        vec![ActionId::Bend, ActionId::Bend, ActionId::Draw],
    );
    // Note: this draft intentionally ends off-target; validation must reject it.
    assert!(valid.validate().is_err());

    let valid = RecipeDraft::from_parts(
        "  Wrought Iron  ",
        "  Tongue Part ",
        0,
        14,
        vec![rule(RuleType::Bend, RuleOrder::Last)],
        vec![ActionId::Bend, ActionId::Bend],
    );
    assert!(valid.validate().is_ok());
    assert_eq!(valid.category, "Wrought Iron");
    assert_eq!(valid.name, "Tongue Part");
    assert_eq!(valid.compact(), "2B");
}
