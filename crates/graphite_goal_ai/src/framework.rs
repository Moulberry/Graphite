use std::{collections::HashMap, hash::Hash};

#[macro_export]
macro_rules! create_goal {
    (
        $goal_name:ident, $macro_name:ident,
        condition_true($($condition_true:ident),*), condition_false($($condition_false:ident),*),
        desired_true($($desired_true:ident),*), desired_false($($desired_false:ident),*) $(,)?
    ) => {
        macro_rules! $macro_name {
            ($framework:expr, $knowledge:ident, $goal:ident) => {
                $framework.register_goal(graphite_goal_ai::framework::GoalFramework {
                    condition_true: vec![$($knowledge::$condition_true),*],
                    condition_false: vec![$($knowledge::$condition_false),*],
                    desired_true: vec![$($knowledge::$desired_true),*],
                    desired_false: vec![$($knowledge::$desired_false),*],
                    goal: $goal::$goal_name,
                });
            };
        }
    };
}

#[macro_export]
macro_rules! create_action {
    (
        $action_name:ident, $macro_name:ident,
        condition_true($($condition_true:ident),*), condition_false($($condition_false:ident),*),
        effect_true($($effect_true:ident),*), effect_false($($effect_false:ident),*),
        effect_toggle($($effect_toggle:ident),*), default_cost($default_cost:expr) $(,)?
    ) => {
        macro_rules! $macro_name {
            ($framework:expr, $knowledge:ident, $action:ident, $cost:expr) => {
                $framework.register_action(graphite_goal_ai::framework::ActionFramework {
                    condition_true: vec![$($knowledge::$condition_true),*],
                    condition_false: vec![$($knowledge::$condition_false),*],
                    effect_true: vec![$($knowledge::$effect_true),*],
                    effect_false: vec![$($knowledge::$effect_false),*],
                    effect_toggle: vec![$($knowledge::$effect_toggle),*],
                    cost: $cost,
                    action: $action::$action_name,
                });
            };
            ($framework:expr, $knowledge:ident, $action:ident) => {
                $framework.register_action(graphite_goal_ai::framework::ActionFramework {
                    condition_true: vec![$($knowledge::$condition_true),*],
                    condition_false: vec![$($knowledge::$condition_false),*],
                    effect_true: vec![$($knowledge::$effect_true),*],
                    effect_false: vec![$($knowledge::$effect_false),*],
                    effect_toggle: vec![$($knowledge::$effect_toggle),*],
                    cost: $default_cost,
                    action: $action::$action_name,
                });
            };
        }
    };
}

pub use create_goal;
pub use create_action;

use crate::goal_engine::{Action, Goal, GoalOrientedActionPlanner};

pub struct GoalFramework<K, G> {
    pub condition_true: Vec<K>,
    pub condition_false: Vec<K>,
    pub desired_true: Vec<K>,
    pub desired_false: Vec<K>,
    pub goal: G
}

pub struct ActionFramework<K, A> {
    pub condition_true: Vec<K>,
    pub condition_false: Vec<K>,
    pub effect_true: Vec<K>,
    pub effect_false: Vec<K>,
    pub effect_toggle: Vec<K>,
    pub cost: f32,
    pub action: A
}

pub struct GoalOrientedActionPlannerFramework<K, G, A>
where
    K: std::fmt::Debug + Clone + Hash + Eq + PartialEq,
    G: std::fmt::Debug + Clone + Hash + Eq + PartialEq,
    A: std::fmt::Debug + Clone + Hash + Eq + PartialEq,
{
    goals: Vec<GoalFramework<K, G>>,
    actions: Vec<ActionFramework<K, A>>,
}

#[derive(Debug)]
pub enum GOAPBuildError<K, G, A> {
    DuplicateGoal(G),
    DuplicateAction(A),
    DuplicateConditionForGoal(G, K),
    DuplicateConditionForAction(A, K),
    DuplicateEffectForAction(A, K),
}

impl <K, G, A> GoalOrientedActionPlannerFramework<K, G, A>
where
    K: std::fmt::Debug + Clone + Hash + Eq + PartialEq,
    G: std::fmt::Debug + Clone + Hash + Eq + PartialEq,
    A: std::fmt::Debug + Clone + Hash + Eq + PartialEq,
{
    pub fn new() -> Self {
        Self {
            goals: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub fn register_goal(&mut self, goal: GoalFramework<K, G>) {
        self.goals.push(goal);
    }

    pub fn register_action(&mut self, action: ActionFramework<K, A>) {
        self.actions.push(action);
    }

    fn get_or_create_knowledge(map: &mut HashMap<K, u32>, knowledge: &K) -> u32 {
        if let Some(knowledge) = map.get(knowledge) {
            *knowledge
        } else {
            let index = map.len() as u32;
            map.insert(knowledge.clone(), index);
            index
        }
    }

    pub fn build(&self) -> Result<GoalOrientedActionPlanner<K, G, A>, GOAPBuildError<K, G, A>> {
        let mut knowledge = HashMap::new();
        let mut goals = Vec::new();
        let mut actions = Vec::new();

        for goal in &self.goals {
            let mut condition_mask = 0_u64;
            let mut condition_cmp = 0_u64;

            for condition_true in &goal.condition_true {
                let index = Self::get_or_create_knowledge(&mut knowledge, condition_true);
                if (condition_mask & (1 << index)) != 0 {
                    return Err(GOAPBuildError::DuplicateConditionForGoal(goal.goal.clone(), condition_true.clone()));
                }
                condition_mask |= 1 << index;
                condition_cmp |= 1 << index;
            }
            for condition_false in &goal.condition_false {
                let index = Self::get_or_create_knowledge(&mut knowledge, condition_false);
                if (condition_mask & (1 << index)) != 0 {
                    return Err(GOAPBuildError::DuplicateConditionForGoal(goal.goal.clone(), condition_false.clone()));
                }
                condition_mask |= 1 << index;
            }

            let mut desired_mask = 0_u64;
            let mut desired_cmp = 0_u64;

            for desired_true in &goal.desired_true {
                let index = Self::get_or_create_knowledge(&mut knowledge, desired_true);
                if (desired_mask & (1 << index)) != 0 {
                    return Err(GOAPBuildError::DuplicateConditionForGoal(goal.goal.clone(), desired_true.clone()));
                }
                desired_mask |= 1 << index;
                desired_cmp |= 1 << index;
            }
            for desired_false in &goal.desired_false {
                let index = Self::get_or_create_knowledge(&mut knowledge, desired_false);
                if (desired_mask & (1 << index)) != 0 {
                    return Err(GOAPBuildError::DuplicateConditionForGoal(goal.goal.clone(), desired_false.clone()));
                }
                desired_mask |= 1 << index;
            }

            goals.push(Goal {
                condition_mask,
                condition_cmp,
                desired_mask,
                desired_cmp,
                goal: goal.goal.clone(),
            })
        }

        for action in &self.actions {
            let mut condition_mask = 0_u64;
            let mut condition_cmp = 0_u64;

            for condition_true in &action.condition_true {
                let index = Self::get_or_create_knowledge(&mut knowledge, condition_true);
                if (condition_mask & (1 << index)) != 0 {
                    return Err(GOAPBuildError::DuplicateConditionForAction(action.action.clone(), condition_true.clone()));
                }
                condition_mask |= 1 << index;
                condition_cmp |= 1 << index;
            }
            for condition_false in &action.condition_false {
                let index = Self::get_or_create_knowledge(&mut knowledge, condition_false);
                if (condition_mask & (1 << index)) != 0 {
                    return Err(GOAPBuildError::DuplicateConditionForAction(action.action.clone(), condition_false.clone()));
                }
                condition_mask |= 1 << index;
            }

            let mut effect_mask = u64::MAX;
            let mut effect_or = 0_u64;
            let mut effect_xor = 0_u64;

            for effect_true in &action.effect_true {
                let index = Self::get_or_create_knowledge(&mut knowledge, effect_true);
                if (effect_or & (1 << index)) != 0 {
                    return Err(GOAPBuildError::DuplicateEffectForAction(action.action.clone(), effect_true.clone()));
                }
                effect_or |= 1 << index;
            }
            for effect_false in &action.effect_false {
                let index = Self::get_or_create_knowledge(&mut knowledge, effect_false);
                if (effect_mask & (1 << index)) == 0 || (effect_or & (1 << index)) != 0 {
                    return Err(GOAPBuildError::DuplicateEffectForAction(action.action.clone(), effect_false.clone()));
                }
                effect_mask &= !(1 << index);
            }
            for effect_toggle in &action.effect_toggle {
                let index = Self::get_or_create_knowledge(&mut knowledge, effect_toggle);
                if (effect_xor & (1 << index)) != 0 || (effect_mask & (1 << index)) == 0 || (effect_or & (1 << index)) != 0 {
                    return Err(GOAPBuildError::DuplicateEffectForAction(action.action.clone(), effect_toggle.clone()));
                }
                effect_xor |= 1 << index;
            }

            actions.push(Action {
                condition_mask,
                condition_cmp,
                effect_mask,
                effect_or,
                effect_xor,
                cost: action.cost,
                action: action.action.clone(),
            })
        }

        let mut goal_types = HashMap::new();
        for (index, goal) in goals.iter().enumerate() {
            if goal_types.contains_key(&goal.goal) {
                return Err(GOAPBuildError::DuplicateGoal(goal.goal.clone()));
            }
            goal_types.insert(goal.goal.clone(), index as u32);
        }

        let mut action_types = HashMap::new();
        for (index, action) in actions.iter().enumerate() {
            if action_types.contains_key(&action.action) {
                return Err(GOAPBuildError::DuplicateAction(action.action.clone()));
            }
            action_types.insert(action.action.clone(), index as u32);
        }

        Ok(GoalOrientedActionPlanner {
            knowledge_types: knowledge,
            goal_types,
            action_types,
            knowledge: 0,
            goals,
            actions,
        })
    }
}