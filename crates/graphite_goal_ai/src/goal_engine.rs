use std::{cmp::Ordering, collections::{BinaryHeap, HashMap}, hash::{BuildHasherDefault, Hash}};

use indexmap::{map::Entry, IndexMap};
use rustc_hash::FxHasher;

#[derive(Debug)]
pub struct Goal<G> {
    pub(crate) condition_mask: u64,
    pub(crate) condition_cmp: u64,
    pub(crate) desired_mask: u64,
    pub(crate) desired_cmp: u64,
    pub(crate) goal: G
}

#[derive(Debug)]
pub struct Action<A> {
    pub(crate) condition_mask: u64,
    pub(crate) condition_cmp: u64,
    pub(crate) effect_mask: u64,
    pub(crate) effect_or: u64,
    pub(crate) effect_xor: u64,
    pub(crate) cost: f32,
    pub(crate) action: A
}

pub struct GoalOrientedActionPlanner<K, G, A>
where
    K: Clone + Hash + Eq + PartialEq,
    G: Clone + Hash + Eq + PartialEq,
    A: Clone + Hash + Eq + PartialEq,
{
    // todo: do we need these?
    pub(crate) knowledge_types: HashMap<K, u32>,
    pub(crate) goal_types: HashMap<G, u32>,
    pub(crate) action_types: HashMap<A, u32>,

    pub(crate) knowledge: u64,
    pub(crate) goals: Vec<Goal<G>>,
    pub(crate) actions: Vec<Action<A>>,
}

struct PlanNode<A: Clone> {
    parent_index: usize,
    cost: f32,
    action: Option<A>
}

impl <K, G, A> GoalOrientedActionPlanner<K, G, A>
where
    K: std::fmt::Debug + Clone + Hash + Eq + PartialEq,
    G: std::fmt::Debug + Clone + Hash + Eq + PartialEq,
    A: std::fmt::Debug + Clone + Hash + Eq + PartialEq,
{
    pub fn set_knowledge(&mut self, knowledge: K, value: bool) {
        if let Some(index) = self.knowledge_types.get(&knowledge) {
            if value {
                self.knowledge |= 1 << index;
            } else {
                self.knowledge &= !(1 << index);
            }
        }
    }

    pub fn set_action_cost(&mut self, action: A, cost: f32) {
        if let Some(index) = self.action_types.get(&action) {
            if let Some(action) = self.actions.get_mut(*index as usize) {
                action.cost = cost;
            }
        }
    }

    pub fn plan(&self) -> Option<(G, A)> {
        let goal = self.choose_goal()?;
        let desired_mask = goal.desired_mask;
        let desired_cmp = goal.desired_cmp;

        let mut to_see = BinaryHeap::new();
        to_see.push(SmallestCostHolder {
            index: 0,
            cost: 0.0,
            estimated_cost: 0.0,
        });

        let mut node_map: IndexMap<u64, PlanNode<A>, BuildHasherDefault<FxHasher>> = IndexMap::default();
        node_map.insert(self.knowledge.clone(), PlanNode {
            parent_index: usize::MAX,
            action: None,
            cost: 0.0,
        });

        let mut count = 0;

        while let Some(holder) = to_see.pop() {
            count += 1;
            if count > 512 {
                return None;
            }

            let (node_flags, node_info) = node_map.get_index(holder.index).unwrap();
            let node_flags = *node_flags;

            if (node_flags & desired_mask) == desired_cmp {
                let mut curr_node = node_info;
                let mut first_action = None;
                while curr_node.parent_index != usize::MAX {
                    if let Some(action) = &curr_node.action {
                        first_action = Some(action);
                    }
                    
                    let (_, parent_node) = node_map.get_index(curr_node.parent_index).unwrap();
                    curr_node = parent_node;
                }
                return Some((goal.goal.clone(), first_action?.clone()));
            }

            let holder_index = holder.index;
            let holder_cost = holder.cost;

            if holder_cost > node_info.cost {
                // todo: is this called often?
                continue;
            }

            for action in &self.actions {
                if (node_flags & action.condition_mask) == action.condition_cmp && action.cost < f32::MAX {
                    let new_cost = holder_cost + action.cost;
                    let new_index;
                    let new_knowledge = ((node_flags & action.effect_mask) | action.effect_or) ^ action.effect_xor;
    
                    match node_map.entry(new_knowledge) {
                        Entry::Occupied(mut e) => {
                            if e.get().cost > new_cost {
                                new_index = e.index();
                                e.insert(PlanNode {
                                    parent_index: holder_index,
                                    cost: new_cost,
                                    action: Some(action.action.clone())
                                });
                            } else {
                                continue;
                            }
                        },
                        Entry::Vacant(v) => {
                            new_index = v.index();
                            v.insert(PlanNode {
                                parent_index: holder_index,
                                cost: new_cost,
                                action: Some(action.action.clone())
                            });
                        },
                    }
    
                    let distance = ((new_knowledge & desired_mask) ^ desired_cmp).count_ones() as f32;
                    let estimated_cost = new_cost + distance;
    
                    to_see.push(SmallestCostHolder {
                        index: new_index,
                        cost: new_cost,
                        estimated_cost,
                    });
                }
            }
        }

        None
    }

    pub fn choose_goal(&self) -> Option<&Goal<G>> {
        for goal in &self.goals {
            if (self.knowledge & goal.condition_mask) == goal.condition_cmp {
                return Some(goal);
            }
        }
        None
    }
}

struct SmallestCostHolder {
    index: usize,
    cost: f32,
    estimated_cost: f32,
}

impl PartialEq for SmallestCostHolder {
    fn eq(&self, other: &Self) -> bool {
        self.estimated_cost.eq(&other.estimated_cost) && self.cost.eq(&other.cost)
    }
}

impl Eq for SmallestCostHolder {}

impl PartialOrd for SmallestCostHolder {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SmallestCostHolder {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match other.estimated_cost.total_cmp(&self.estimated_cost) {
            std::cmp::Ordering::Equal => self.cost.total_cmp(&other.cost),
            ordering => ordering,
        }
    }
}