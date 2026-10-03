//! Core domain types and constants for the TerraFirmaCraft forge planner.
//!
//! This is a faithful Rust port of the original TypeScript `src/domain.ts`. All
//! values are derived from the TFC 1.20.1 v3.2.25 source at tag commit
//! `b158c9c968cad7afb99cea2c937b301ff275e2e6`:
//!
//!   - `ForgeStep.java` (`LIMIT = 150`, step deltas)
//!   - `ForgeRule.java` (rule semantics)
//!
//! The authoritative, source-confirmed work range is INCLUSIVE `0..=150`. Any
//! intermediate work value outside that range would destroy the item in game.

/// Inclusive upper bound of the legal work range. `ForgeStep.LIMIT = 150`.
pub const LIMIT: i32 = 150;

/// Inclusive lower bound of the legal work range.
pub const MIN_WORK: i32 = 0;

/// One of the eight anvil actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionId {
    HitLight,
    HitMedium,
    HitHard,
    Draw,
    Punch,
    Bend,
    Upset,
    Shrink,
}

/// Generic rule type. A generic [`RuleType::Hit`] covers all three hit strengths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleType {
    Hit,
    Draw,
    Punch,
    Bend,
    Upset,
    Shrink,
}

/// History position a rule is satisfied in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleOrder {
    Any,
    Last,
    NotLast,
    SecondLast,
    ThirdLast,
}

/// A single forge rule: a generic type plus an order constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForgeRule {
    pub rule_type: RuleType,
    pub order: RuleOrder,
}

impl ForgeRule {
    pub fn new(rule_type: RuleType, order: RuleOrder) -> Self {
        Self { rule_type, order }
    }
}

/// An ordered solution returned by the solver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeSolution {
    /// Ordered actions applied from start to target.
    pub actions: Vec<ActionId>,
    /// Visited numeric work values, including start and target.
    pub path: Vec<i32>,
    /// Compact notation, e.g. `"1U 3S 1U 3S 1B"`.
    pub compact: String,
}

/// Static definition of an anvil action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionDef {
    pub id: ActionId,
    pub label: &'static str,
    /// Signed work delta applied by this action.
    pub delta: i32,
    /// Compact-notation symbol.
    pub symbol: &'static str,
    /// Rule type this action satisfies.
    pub rule_type: RuleType,
}

/// Canonical action order. The solver iterates in this order, which makes BFS
/// results deterministic across runs.
pub const ACTIONS: [ActionDef; 8] = [
    ActionDef {
        id: ActionId::HitLight,
        label: "Light Hit",
        delta: -3,
        symbol: "LH",
        rule_type: RuleType::Hit,
    },
    ActionDef {
        id: ActionId::HitMedium,
        label: "Medium Hit",
        delta: -6,
        symbol: "MD",
        rule_type: RuleType::Hit,
    },
    ActionDef {
        id: ActionId::HitHard,
        label: "Hard Hit",
        delta: -9,
        symbol: "HH",
        rule_type: RuleType::Hit,
    },
    ActionDef {
        id: ActionId::Draw,
        label: "Draw",
        delta: -15,
        symbol: "D",
        rule_type: RuleType::Draw,
    },
    ActionDef {
        id: ActionId::Punch,
        label: "Punch",
        delta: 2,
        symbol: "P",
        rule_type: RuleType::Punch,
    },
    ActionDef {
        id: ActionId::Bend,
        label: "Bend",
        delta: 7,
        symbol: "B",
        rule_type: RuleType::Bend,
    },
    ActionDef {
        id: ActionId::Upset,
        label: "Upset",
        delta: 13,
        symbol: "U",
        rule_type: RuleType::Upset,
    },
    ActionDef {
        id: ActionId::Shrink,
        label: "Shrink",
        delta: 16,
        symbol: "S",
        rule_type: RuleType::Shrink,
    },
];

pub const RULE_TYPES: [RuleType; 6] = [
    RuleType::Hit,
    RuleType::Draw,
    RuleType::Punch,
    RuleType::Bend,
    RuleType::Upset,
    RuleType::Shrink,
];

pub const RULE_ORDERS: [RuleOrder; 5] = [
    RuleOrder::Any,
    RuleOrder::Last,
    RuleOrder::NotLast,
    RuleOrder::SecondLast,
    RuleOrder::ThirdLast,
];

/// Look up the static definition for an action.
pub fn action_def(id: ActionId) -> &'static ActionDef {
    ACTIONS
        .iter()
        .find(|action| action.id == id)
        .expect("every ActionId has a definition")
}

impl ActionId {
    /// Signed work delta applied by this action.
    pub fn delta(self) -> i32 {
        action_def(self).delta
    }

    /// Compact-notation symbol.
    pub fn symbol(self) -> &'static str {
        action_def(self).symbol
    }

    /// Rule type this action satisfies.
    pub fn rule_type(self) -> RuleType {
        action_def(self).rule_type
    }
}

impl RuleType {
    pub fn label(self) -> &'static str {
        match self {
            RuleType::Hit => "Hit",
            RuleType::Draw => "Draw",
            RuleType::Punch => "Punch",
            RuleType::Bend => "Bend",
            RuleType::Upset => "Upset",
            RuleType::Shrink => "Shrink",
        }
    }
}

impl RuleOrder {
    pub fn label(self) -> &'static str {
        match self {
            RuleOrder::Any => "Any",
            RuleOrder::Last => "Last",
            RuleOrder::NotLast => "Not Last",
            RuleOrder::SecondLast => "Second Last",
            RuleOrder::ThirdLast => "Third Last",
        }
    }
}

/// Database stable key for an action id (snake_case).
impl ActionId {
    pub fn as_str(self) -> &'static str {
        match self {
            ActionId::HitLight => "hit_light",
            ActionId::HitMedium => "hit_medium",
            ActionId::HitHard => "hit_hard",
            ActionId::Draw => "draw",
            ActionId::Punch => "punch",
            ActionId::Bend => "bend",
            ActionId::Upset => "upset",
            ActionId::Shrink => "shrink",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "hit_light" => Some(ActionId::HitLight),
            "hit_medium" => Some(ActionId::HitMedium),
            "hit_hard" => Some(ActionId::HitHard),
            "draw" => Some(ActionId::Draw),
            "punch" => Some(ActionId::Punch),
            "bend" => Some(ActionId::Bend),
            "upset" => Some(ActionId::Upset),
            "shrink" => Some(ActionId::Shrink),
            _ => None,
        }
    }
}

impl RuleType {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleType::Hit => "hit",
            RuleType::Draw => "draw",
            RuleType::Punch => "punch",
            RuleType::Bend => "bend",
            RuleType::Upset => "upset",
            RuleType::Shrink => "shrink",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "hit" => Some(RuleType::Hit),
            "draw" => Some(RuleType::Draw),
            "punch" => Some(RuleType::Punch),
            "bend" => Some(RuleType::Bend),
            "upset" => Some(RuleType::Upset),
            "shrink" => Some(RuleType::Shrink),
            _ => None,
        }
    }
}

impl RuleOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleOrder::Any => "any",
            RuleOrder::Last => "last",
            RuleOrder::NotLast => "not_last",
            RuleOrder::SecondLast => "second_last",
            RuleOrder::ThirdLast => "third_last",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "any" => Some(RuleOrder::Any),
            "last" => Some(RuleOrder::Last),
            "not_last" => Some(RuleOrder::NotLast),
            "second_last" => Some(RuleOrder::SecondLast),
            "third_last" => Some(RuleOrder::ThirdLast),
            _ => None,
        }
    }
}

/// One of the three history positions a rule can be satisfied in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryPosition {
    Last,
    SecondLast,
    ThirdLast,
}

/// Exact set of history positions each order is compatible with.
fn order_positions(order: RuleOrder) -> &'static [HistoryPosition] {
    use HistoryPosition::*;
    match order {
        RuleOrder::Last => &[Last],
        RuleOrder::SecondLast => &[SecondLast],
        RuleOrder::ThirdLast => &[ThirdLast],
        RuleOrder::NotLast => &[SecondLast, ThirdLast],
        RuleOrder::Any => &[Last, SecondLast, ThirdLast],
    }
}

/// True exactly when every rule can be assigned to a distinct one of the three
/// history positions it allows. This is a small deterministic backtracking
/// matcher over at most three rules and three positions.
pub fn has_distinct_rule_order_slots(rules: &[ForgeRule]) -> bool {
    if rules.len() > 3 {
        return false;
    }
    let mut used: Vec<HistoryPosition> = Vec::with_capacity(3);

    fn assign(index: usize, rules: &[ForgeRule], used: &mut Vec<HistoryPosition>) -> bool {
        if index == rules.len() {
            return true;
        }
        for &position in order_positions(rules[index].order) {
            if used.contains(&position) {
                continue;
            }
            used.push(position);
            if assign(index + 1, rules, used) {
                return true;
            }
            used.pop();
        }
        false
    }

    assign(0, rules, &mut used)
}

/// Groups consecutive equal actions and prefixes each run with its count.
pub fn compact_actions(actions: &[ActionId]) -> String {
    let mut tokens: Vec<String> = Vec::new();
    let mut index = 0;
    while index < actions.len() {
        let action = actions[index];
        let mut count = 1;
        while index + count < actions.len() && actions[index + count] == action {
            count += 1;
        }
        tokens.push(format!("{count}{}", action.symbol()));
        index += count;
    }
    tokens.join(" ")
}

/// Numeric path visited when applying `actions` starting from `start`.
pub fn path_from_actions(start: i32, actions: &[ActionId]) -> Vec<i32> {
    let mut path = Vec::with_capacity(actions.len() + 1);
    path.push(start);
    let mut work = start;
    for &action in actions {
        work += action.delta();
        path.push(work);
    }
    path
}

/// A named solution persisted by the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedRecipe {
    pub id: String,
    pub category: String,
    pub name: String,
    pub start: i32,
    pub target: i32,
    pub rules: Vec<ForgeRule>,
    pub actions: Vec<ActionId>,
    pub compact: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A recipe submitted for storage, before an id/timestamps are assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeDraft {
    pub category: String,
    pub name: String,
    pub start: i32,
    pub target: i32,
    pub rules: Vec<ForgeRule>,
    pub actions: Vec<ActionId>,
}

impl RecipeDraft {
    /// Build a draft from the raw submitted fields, normalising category/name
    /// (trim) and compact notation (derived from `actions`).
    ///
    /// This mirrors the original server `parseRecipeInput`: the submitted
    /// `compact` string is ignored and always recomputed from the actions.
    pub fn from_parts(
        category: impl Into<String>,
        name: impl Into<String>,
        start: i32,
        target: i32,
        rules: Vec<ForgeRule>,
        actions: Vec<ActionId>,
    ) -> Self {
        Self {
            category: category.into().trim().to_string(),
            name: name.into().trim().to_string(),
            start,
            target,
            rules,
            actions,
        }
    }

    /// Strict validation matching the original server invariants.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let category_trimmed = self.category.trim();
        if category_trimmed.is_empty() || self.category.chars().count() > 100 {
            return Err(ValidationError::Category);
        }
        let name_trimmed = self.name.trim();
        if name_trimmed.is_empty() || self.name.chars().count() > 200 {
            return Err(ValidationError::Name);
        }
        if !(MIN_WORK..=LIMIT).contains(&self.start) || !(MIN_WORK..=LIMIT).contains(&self.target) {
            return Err(ValidationError::Range);
        }
        if self.rules.is_empty() || self.rules.len() > 3 {
            return Err(ValidationError::Rules);
        }
        if !has_distinct_rule_order_slots(&self.rules) {
            return Err(ValidationError::RuleSlots);
        }
        if self.actions.len() > 256 {
            return Err(ValidationError::Actions);
        }

        let path = path_from_actions(self.start, &self.actions);
        if path.first().copied() != Some(self.start) || path.last().copied() != Some(self.target) {
            return Err(ValidationError::Actions);
        }
        if path.iter().any(|work| !(MIN_WORK..=LIMIT).contains(work)) {
            return Err(ValidationError::Actions);
        }
        Ok(())
    }

    /// Canonical compact notation for this draft.
    pub fn compact(&self) -> String {
        compact_actions(&self.actions)
    }
}

/// Validation failures for a [`RecipeDraft`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("category must be a non-empty string of at most 100 characters")]
    Category,
    #[error("name must be a non-empty string of at most 200 characters")]
    Name,
    #[error("start and target must be integers within 0..150")]
    Range,
    #[error("rules must contain between 1 and 3 valid forge rules")]
    Rules,
    #[error("rules must be assignable to distinct last, second-last and third-last positions")]
    RuleSlots,
    #[error(
        "actions must be an array of 0..=256 valid action ids that move start to target within 0..150"
    )]
    Actions,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_pinned_slots() {
        assert!(!has_distinct_rule_order_slots(&[
            ForgeRule::new(RuleType::Hit, RuleOrder::Last),
            ForgeRule::new(RuleType::Draw, RuleOrder::Last),
        ]));
        assert!(!has_distinct_rule_order_slots(&[
            ForgeRule::new(RuleType::Hit, RuleOrder::SecondLast),
            ForgeRule::new(RuleType::Draw, RuleOrder::SecondLast),
        ]));
        assert!(!has_distinct_rule_order_slots(&[
            ForgeRule::new(RuleType::Hit, RuleOrder::ThirdLast),
            ForgeRule::new(RuleType::Draw, RuleOrder::ThirdLast),
        ]));
    }

    #[test]
    fn allows_valid_multi_rule_sets() {
        assert!(has_distinct_rule_order_slots(&[
            ForgeRule::new(RuleType::Hit, RuleOrder::Any),
            ForgeRule::new(RuleType::Draw, RuleOrder::Any),
            ForgeRule::new(RuleType::Bend, RuleOrder::Any),
        ]));
        assert!(has_distinct_rule_order_slots(&[
            ForgeRule::new(RuleType::Hit, RuleOrder::NotLast),
            ForgeRule::new(RuleType::Draw, RuleOrder::NotLast),
        ]));
        assert!(has_distinct_rule_order_slots(&[
            ForgeRule::new(RuleType::Punch, RuleOrder::Last),
            ForgeRule::new(RuleType::Punch, RuleOrder::Any),
        ]));
        assert!(has_distinct_rule_order_slots(&[]));
        assert!(has_distinct_rule_order_slots(&[ForgeRule::new(
            RuleType::Hit,
            RuleOrder::NotLast
        )]));
    }

    #[test]
    fn rejects_impossible_sets() {
        assert!(!has_distinct_rule_order_slots(&[
            ForgeRule::new(RuleType::Hit, RuleOrder::NotLast),
            ForgeRule::new(RuleType::Bend, RuleOrder::NotLast),
            ForgeRule::new(RuleType::Draw, RuleOrder::NotLast),
        ]));
        assert!(!has_distinct_rule_order_slots(&[
            ForgeRule::new(RuleType::Hit, RuleOrder::SecondLast),
            ForgeRule::new(RuleType::Bend, RuleOrder::ThirdLast),
            ForgeRule::new(RuleType::Draw, RuleOrder::NotLast),
        ]));
    }

    #[test]
    fn compact_groups_consecutive_actions() {
        let actions = [
            ActionId::Upset,
            ActionId::Shrink,
            ActionId::Shrink,
            ActionId::Shrink,
            ActionId::Upset,
            ActionId::Shrink,
            ActionId::Shrink,
            ActionId::Shrink,
            ActionId::Bend,
        ];
        assert_eq!(compact_actions(&actions), "1U 3S 1U 3S 1B");
        assert_eq!(compact_actions(&[]), "");
        assert_eq!(
            compact_actions(&[ActionId::HitLight, ActionId::HitMedium, ActionId::HitHard]),
            "1LH 1MD 1HH"
        );
    }

    #[test]
    fn action_deltas_are_source_correct() {
        assert_eq!(ActionId::HitLight.delta(), -3);
        assert_eq!(ActionId::HitMedium.delta(), -6);
        assert_eq!(ActionId::HitHard.delta(), -9);
        assert_eq!(ActionId::Draw.delta(), -15);
        assert_eq!(ActionId::Punch.delta(), 2);
        assert_eq!(ActionId::Bend.delta(), 7);
        assert_eq!(ActionId::Upset.delta(), 13);
        assert_eq!(ActionId::Shrink.delta(), 16);
        assert_eq!(ACTIONS.len(), 8);
        assert_eq!(LIMIT, 150);
    }
}
