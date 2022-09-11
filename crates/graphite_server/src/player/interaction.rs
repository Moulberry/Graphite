use graphite_mc_protocol::types::{BlockPosition, Direction, Hand};

#[derive(Debug)]
pub enum Interaction {
    // Simple left click
    LeftClickBlock {
        position: BlockPosition,
        face: Direction,
        instabreak: bool,
    },
    LeftClickEntity {
        entity_id: i32,
    },
    LeftClickAir,

    // Simple right click
    RightClickBlock {
        position: BlockPosition,
        face: Direction,
        offset: (f32, f32, f32),
    },
    RightClickEntity {
        entity_id: i32,
        offset: (f32, f32, f32),
    },
    RightClickAir {
        hand: Hand,
    },

    // Breaking
    ContinuousBreak {
        position: BlockPosition,
        break_time: usize, // Number of server ticks that have been spent breaking this block
        distance: f32,     // Distance to block
    },
    FinishBreak {
        position: BlockPosition,
        break_time: usize, // Number of server ticks that have been spent breaking this block
        distance: f32,     // Distance to block
    },
    AbortBreak {
        position: BlockPosition,
        break_time: usize, // Number of server ticks that have been spent breaking this block
    },

    // Using
    ContinuousUse {
        use_time: usize,
        hand: Hand,
    },
    FinishUse {
        use_time: usize,
        hand: Hand,
    },
    AbortUse {
        use_time: usize,
        hand: Hand,
        aborted_by_client: bool,
    },
}

/*#[derive(Debug)]
pub enum InteractionTarget {
    Block {
        position: BlockPosition,
        face: Option<Direction>,
        offset: Option<(f32, f32, f32)> // present if InteractionType == RightClick
    },
    Entity {
        entity_id: i32
    },
    Air
}

#[derive(Debug)]
pub struct Interaction {
    pub target: InteractionTarget,
    pub click: InteractionType,
}*/

#[derive(Default)]
pub(crate) struct InteractionState {
    pub(crate) ignore_swing_ticks: usize,

    pub(crate) processed_use_item_mainhand: bool,
    pub(crate) processed_use_item_offhand: bool,
    pub(crate) processed_interaction: bool,

    pub(crate) using_hand: Option<Hand>,
    pub(crate) use_time: usize,
    pub(crate) max_use_time: Option<usize>,

    pub(crate) breaking_block: Option<BlockPosition>,
    pub(crate) break_time: usize,
    // Used in order to reset breaking_block if the player stops breaking for long enough
    pub(crate) breaking_block_timer: usize,
}

impl InteractionState {
    pub(crate) fn reset(&mut self) {
        // Using
        self.use_time = 0;
        self.max_use_time = None;
        self.using_hand = None;

        // Breaking block
        self.breaking_block = None;
        self.break_time = 0;
        self.breaking_block_timer = 0;
    }

    pub(crate) fn start_breaking(&mut self, position: BlockPosition) -> Option<Interaction> {
        let ret = self.try_abort_break_or_use();

        self.breaking_block_timer = 5;
        self.breaking_block = Some(position);

        ret
    }

    pub(crate) fn try_abort_break_or_use(&mut self) -> Option<Interaction> {
        let abort_break = self.try_abort_break();
        if abort_break.is_some() {
            debug_assert!(self.using_hand.is_none()); // Can't break and use simultaneously
            abort_break
        } else {
            self.try_abort_use(false)
        }
    }

    pub(crate) fn try_abort_break(&mut self) -> Option<Interaction> {
        if let Some(position) = self.breaking_block {
            let interaction = Interaction::AbortBreak {
                position,
                break_time: self.break_time,
            };

            self.reset();

            Some(interaction)
        } else {
            None
        }
    }

    pub(crate) fn try_finish_break(&mut self, distance: f32) -> Option<Interaction> {
        if let Some(position) = self.breaking_block {
            let interaction = Interaction::FinishBreak {
                position,
                break_time: self.break_time,
                distance,
            };

            self.reset();

            Some(interaction)
        } else {
            None
        }
    }

    pub(crate) fn start_using(&mut self, max_use_time: usize, hand: Hand) -> Option<Interaction> {
        let ret = self.try_abort_break_or_use();

        self.using_hand = Some(hand);
        self.use_time = 0;

        if max_use_time < 1200 {
            self.max_use_time = Some(max_use_time);
        }

        ret
    }

    pub(crate) fn try_abort_use(&mut self, aborted_by_client: bool) -> Option<Interaction> {
        if let Some(hand) = self.using_hand {
            let interaction = if aborted_by_client && self.max_use_time.is_none() {
                Interaction::FinishUse {
                    use_time: self.use_time,
                    hand,
                }
            } else {
                Interaction::AbortUse {
                    use_time: self.use_time,
                    hand,
                    aborted_by_client,
                }
            };

            self.reset();

            Some(interaction)
        } else {
            None
        }
    }

    fn try_finish_use(&mut self) -> Option<Interaction> {
        if let Some(hand) = self.using_hand {
            let interaction = Interaction::FinishUse {
                use_time: self.use_time,
                hand,
            };

            self.reset();

            Some(interaction)
        } else {
            None
        }
    }

    pub(crate) fn get_used_hand(&self) -> Option<Hand> {
        if self.processed_use_item_offhand {
            Some(Hand::Off)
        } else if self.processed_use_item_mainhand {
            Some(Hand::Main)
        } else {
            None
        }
    }

    pub(crate) fn update(&mut self) -> Vec<Interaction> {
        // todo: Option<...> instead of Vec<...>
        let mut interactions = Vec::new();

        self.processed_use_item_mainhand = false;
        self.processed_use_item_offhand = false;
        self.processed_interaction = false;

        if self.ignore_swing_ticks > 0 {
            self.ignore_swing_ticks -= 1;
        }

        if self.breaking_block_timer > 0 {
            debug_assert!(self.using_hand.is_none()); // can't use and break simultaneously

            self.breaking_block_timer -= 1;
            if self.breaking_block_timer == 0 {
                // Player took too long. Abort the break
                interactions.push(self.try_abort_break().expect("break must be active"));
            } else {
                // Still breaking, increase the break time
                self.break_time += 1;
            }
        } else if let Some(hand) = self.using_hand {
            self.use_time += 1;

            if let Some(max_use_time) = self.max_use_time {
                if self.use_time >= max_use_time {
                    interactions.push(self.try_finish_use().expect("use must be active"));
                }
            }

            if self.using_hand.is_some() {
                interactions.push(Interaction::ContinuousUse {
                    use_time: self.use_time,
                    hand,
                })
            }
        }

        interactions
    }
}
