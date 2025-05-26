use std::borrow::Cow;

use graphite_mc_constants::builtin::Attribute;
use graphite_mc_protocol::{play::{self, clientbound::{AttributeEntry, AttributeModifier}}, IdentifiedPacket};
use graphite_network::PacketBuffer;
use strum::IntoEnumIterator;

pub struct SyncableAttributeMap {
    dirty: bool,
    last: SyncableAttributeState,
    current: SyncableAttributeState
}

#[derive(Clone)]
struct Modifier {
    active: bool,
}

#[derive(Clone)]
struct SyncableAttributeState {
    base_values: [f64; Attribute::COUNT],
    modifier_sprinting: Modifier
}

impl SyncableAttributeMap {
    pub fn new() -> Self {
        let mut state = SyncableAttributeState {
            base_values: [0.0_f64; Attribute::COUNT],
            modifier_sprinting: Modifier { active: false },
        };

        for attribute in Attribute::iter() {
            state.base_values[attribute as usize] = attribute.default_value() as f64;
        }

        Self {
            dirty: false,
            last: state.clone(),
            current: state
        }

    }

    pub fn set_base_value(&mut self, attribute: Attribute, base_value: f64) {
        if self.current.base_values[attribute as usize] != base_value {
            self.current.base_values[attribute as usize] = base_value;
            self.dirty = true;
        }
    }

    pub fn get(&self, attribute: Attribute) -> f64 {
        let mut value = self.current.base_values[attribute as usize];
        match attribute {
            Attribute::MovementSpeed => {
                if self.current.modifier_sprinting.active {
                    value += value * 0.3;
                }
            }
            _ => {}
        }
        value
    }

    pub fn set_modifier_sprinting(&mut self, sprinting: bool) {
        self.current.modifier_sprinting.active = sprinting;
        self.dirty = true;
    }

    pub fn get_modifier_sprinting(&self) -> bool {
        self.current.modifier_sprinting.active
    }

    pub fn process_changes(&mut self, force_send_movement_speed: bool) -> Vec<AttributeEntry<'static>> {
        if !self.dirty {
            return Vec::new();
        }
        self.dirty = false;

        let mut attribute_changes = Vec::new();

        for syncable in Attribute::iter() {
            let index = syncable as usize;
            let mut changed = self.last.base_values[index] != self.current.base_values[index];

            if force_send_movement_speed && syncable == Attribute::MovementSpeed {
                changed = true;
            }

            let mut modifiers = Vec::new();

            match syncable {
                Attribute::MovementSpeed => {
                    if self.last.modifier_sprinting.active != self.current.modifier_sprinting.active {
                        changed = true;
                    }
                    if self.current.modifier_sprinting.active {
                        modifiers.push(AttributeModifier {
                            id: Cow::Borrowed("minecraft:sprinting"),
                            amount: 0.3,
                            operation: 2, // add multiplied total
                        });
                    }
                }
                _ => {}
            }

            if changed {
                attribute_changes.push(AttributeEntry {
                    id: syncable,
                    value: self.current.base_values[syncable as usize],
                    modifiers,
                });
            }
        }

        if !attribute_changes.is_empty() {
            self.last = self.current.clone();
        }

        attribute_changes
    }
}