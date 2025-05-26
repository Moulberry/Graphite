use std::ops::{AddAssign, Mul, MulAssign, SubAssign};

use enum_map::EnumMap;
use glam::{DVec2, DVec3, IVec3};
use graphite_binary::slice_serialization::{slice_serializable, BigEndian, Single, SliceSerializable};
use graphite_mc_constants::{builtin::MobEffect, item::Item, types::Pose};
use graphite_mc_protocol::types::ItemStack;
use rustc_hash::FxHashMap;

use super::{Player, PlayerExtension};

#[derive(Debug, Clone, Default)]
pub struct UncertainVelocity {
    known: DVec3,
    min_x: f64,
    min_z: f64,
    max_x: f64,
    max_z: f64,
    pub ignored_item_slow: bool
}

impl MulAssign<DVec3> for UncertainVelocity {
    fn mul_assign(&mut self, rhs: DVec3) {
        self.known *= rhs;
        self.min_x *= rhs.x;
        self.max_x *= rhs.x;
        self.min_z *= rhs.z;
        self.max_z *= rhs.z;
        self.check_nan_or_inf();
    }
}

impl MulAssign<f64> for UncertainVelocity {
    fn mul_assign(&mut self, rhs: f64) {
        self.known *= rhs;
        self.min_x *= rhs;
        self.max_x *= rhs;
        self.min_z *= rhs;
        self.max_z *= rhs;
        self.check_nan_or_inf();
    }
}

impl AddAssign<DVec3> for UncertainVelocity {
    fn add_assign(&mut self, rhs: DVec3) {
        self.known += rhs;
        self.min_x += rhs.x;
        self.max_x += rhs.x;
        self.min_z += rhs.z;
        self.max_z += rhs.z;
        self.check_nan_or_inf();
    }
}

impl UncertainVelocity {
    #[inline(always)]
    fn check_nan_or_inf(&self) {
        if !crate::debug::CHECK_NAN_OR_INF {
            return;
        }
        if !self.known.is_finite() || !self.min_x.is_finite() || !self.max_x.is_finite() || !self.min_z.is_finite() || !self.max_z.is_finite() {
            panic!("UncertainVelocity has nan or inf: {:?}", self);
        }
    }

    pub fn get_min(&self) -> DVec3 {
        DVec3::new(self.min_x, self.known.y, self.min_z)
    }

    pub fn get_max(&self) -> DVec3 {
        DVec3::new(self.max_x, self.known.y, self.max_z)
    }

    pub fn get_expected(&self) -> DVec3 {
        self.known
    }

    pub fn get_y(&self) -> f64 {
        self.known.y
    }

    pub fn has_uncertainty(&self) -> bool {
        self.min_x != self.known.x || self.max_x != self.known.x || self.min_z != self.known.z || self.max_z != self.known.z
    }

    pub fn expand_xz_uncertainty_to_zero(&mut self) {
        self.expand_xz_uncertainty_to(0.0, 0.0);
    }

    pub fn add_horizontal_uncertainty(&mut self, value: f64) {
        self.min_x -= value;
        self.min_z -= value;
        self.max_x += value;
        self.max_z += value;
        self.check_nan_or_inf();
    }

    pub fn maybe_multiply(&mut self, scale_x: f64, scale_z: f64) {
        self.min_x = self.min_x.min(self.min_x * scale_x);
        self.max_x = self.max_x.max(self.max_x * scale_x);
        self.min_z = self.min_z.min(self.min_z * scale_z);
        self.max_z = self.max_z.max(self.max_z * scale_z);
        self.check_nan_or_inf();
    }

    pub fn max_y(&mut self, value: f64) {
        self.known.y = self.known.y.max(value);
        self.check_nan_or_inf();
    }

    pub fn set_zero(&mut self) {
        self.known = DVec3::ZERO;
        self.min_x = 0.0;
        self.max_x = 0.0;
        self.min_z = 0.0;
        self.max_z = 0.0;
        self.check_nan_or_inf();
    }

    pub fn set_x(&mut self, value: f64) {
        self.known.x = value;
        self.min_x = value;
        self.max_x = value;
        self.check_nan_or_inf();
    }

    pub fn set_y(&mut self, value: f64) {
        self.known.y = value;
        self.check_nan_or_inf();
    }

    pub fn set_z(&mut self, value: f64) {
        self.known.z = value;
        self.min_z = value;
        self.max_z = value;
        self.check_nan_or_inf();
    }

    pub fn set_by_axis(&mut self, value: f64, axis: usize) {
        match axis {
            0 => self.set_x(value),
            1 => self.set_y(value),
            _ => self.set_z(value),
        }
    }

    pub fn add_x(&mut self, value: f64) {
        self.known.x += value;
        self.min_x += value;
        self.max_x += value;
        self.check_nan_or_inf();
    }

    pub fn add_y(&mut self, value: f64) {
        self.known.y += value;
        self.check_nan_or_inf();
    }

    pub fn add_z(&mut self, value: f64) {
        self.known.z += value;
        self.min_z += value;
        self.max_z += value;
        self.check_nan_or_inf();
    }

    pub fn add_by_axis(&mut self, value: f64, axis: usize) {
        match axis {
            0 => self.add_x(value),
            1 => self.add_y(value),
            _ => self.add_z(value),
        }
    }

    pub fn set_with_min_max(&mut self, known: DVec3, min: DVec3, max: DVec3) {
        self.known = known;
        self.min_x = min.x.min(known.x).min(max.x);
        self.max_x = min.x.max(known.x).max(max.x);
        self.min_z = min.z.min(known.z).min(max.z);
        self.max_z = min.z.max(known.z).max(max.z);
        self.check_nan_or_inf();
    }

    pub fn expand_xz_uncertainty_to(&mut self, to_x: f64, to_z: f64) {
        self.min_x = self.min_x.min(to_x);
        self.max_x = self.max_x.max(to_x);
        self.min_z = self.min_z.min(to_z);
        self.max_z = self.max_z.max(to_z);
        self.check_nan_or_inf();
    }

    pub fn modify(&mut self, func: impl Fn(&mut DVec3)) {
        if !self.has_uncertainty() {
            (func)(&mut self.known);
            self.min_x = self.known.x;
            self.min_z = self.known.z;
            self.max_x = self.known.x;
            self.max_z = self.known.z;
            return;
        }

        let mut min =  DVec3::new(self.min_x, self.known.y, self.min_z);
        let mut max = DVec3::new(self.max_x, self.known.y, self.max_z);
        
        (func)(&mut min);
        (func)(&mut max);
        (func)(&mut self.known);

        self.set_with_min_max(self.known, min, max);
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(super) enum KnownClientStateChangeKey {
    StateSyncId(u16),
    TeleportId(i32)
}

pub(super) enum KnownClientStateChange {
    SetPose(Pose),
    TeleportPosition {
        position: DVec3,
        relative_coords: u8,
    },
    TeleportVelocity {
        velocity: DVec3,
        relative_coords: u8
    },
    SetLivingFlags {
        flags: u8,
        use_item: Item
    },
    SetSharedFlags(u8),
    SetHeldItemStack {
        held_item_stack: ItemStack,
    },
    UpdateAbilities {
        is_flying: bool,
        can_fly: bool,
        flying_speed: f32
    },
    SetSneakingSpeed(f64),
    SetMovementSpeed {
        speed: f64,
        sprint_modifier: bool
    },
    SetJumpStrength(f64),
    SetGravity(f64),
    SetStepHeight(f64),
    SetWaterMovementEfficiency(f64),
    AcknowledgeBlockUpdates {
        positions: Vec<IVec3>
    }
}

impl KnownClientStateChange {
    pub(super) fn apply<P: PlayerExtension>(self, player: &mut Player<P>) {
        let state = &mut player.known_client_state;
        match self {
            KnownClientStateChange::SetPose(pose) => {
                state.pose = pose;
            },
            KnownClientStateChange::TeleportPosition { position, relative_coords } => {
                if relative_coords == 0 && state.awaiting_absolute_teleport_count > 0 {
                    state.awaiting_absolute_teleport_count -= 1;
                }

                if state.awaiting_absolute_teleport_count == 0 {
                    for axis in 0..3 {
                        if (relative_coords & (1 << axis)) != 0 {
                            player.position[axis] += position[axis];
                            player.last_client_position[axis] += position[axis];
                        } else {
                            player.position[axis] = position[axis];
                            player.last_client_position[axis] = position[axis];
                        }
                    }
                }

            },
            KnownClientStateChange::TeleportVelocity { velocity, relative_coords } => {
                for axis in 0..3 {
                    if (relative_coords & (1 << axis)) != 0 {
                        state.velocity.add_by_axis(velocity[axis], axis);
                    } else {
                        state.velocity.set_by_axis(velocity[axis], axis);
                    }
                }
            },
            KnownClientStateChange::SetLivingFlags { flags, use_item } => {
                state.living_flags = flags;
                state.use_item = use_item;
            },
            KnownClientStateChange::SetSharedFlags(flags) => {
                state.shared_flags = flags;
            },
            KnownClientStateChange::SetHeldItemStack { held_item_stack } => {
                state.held_item_stack = held_item_stack;
            }
            KnownClientStateChange::UpdateAbilities { is_flying, can_fly, flying_speed } => {
                state.is_flying = is_flying;
                state.is_flying_pre_tick = is_flying;
                state.can_fly = can_fly;
                state.flying_speed = flying_speed;
            }
            KnownClientStateChange::SetSneakingSpeed(sneaking_speed) => {
                state.sneaking_speed = sneaking_speed;
            },
            KnownClientStateChange::SetMovementSpeed { speed, sprint_modifier } => {
                state.movement_speed = speed;
                state.applied_sprint_modifier_to_movement_speed = sprint_modifier;
            },
            KnownClientStateChange::SetJumpStrength(jump_strength) => {
                state.jump_strength = jump_strength;
            },
            KnownClientStateChange::SetGravity(gravity) => {
                state.gravity = gravity;
            },
            KnownClientStateChange::SetStepHeight(step_height) => {
                state.step_height = step_height;
            },
            KnownClientStateChange::SetWaterMovementEfficiency(water_movement_efficiency) => {
                state.water_movement_efficiency = water_movement_efficiency;
            },
            KnownClientStateChange::AcknowledgeBlockUpdates { mut positions } => {
                for position in positions.drain(..) {
                    state.known_blocks.remove(&position);
                }
            }
        }
    }
}

struct DVec3Serializer;
impl <'r, 'd: 'r> SliceSerializable<'r, 'd, DVec3> for DVec3Serializer {
    type CopyType = DVec3;

    fn as_copy_type(t: &DVec3) -> Self::CopyType {
        *t
    }

    fn read(bytes: &mut &[u8]) -> anyhow::Result<DVec3> {
        let x = <BigEndian as SliceSerializable<f64>>::read(bytes)?;
        let y = <BigEndian as SliceSerializable<f64>>::read(bytes)?;
        let z = <BigEndian as SliceSerializable<f64>>::read(bytes)?;
        Ok(DVec3::new(x, y, z))
    }

    unsafe fn write(mut bytes: &mut [u8], data: DVec3) -> &mut [u8] {
        bytes = <BigEndian as SliceSerializable<f64>>::write(bytes, data.x);
        bytes = <BigEndian as SliceSerializable<f64>>::write(bytes, data.y);
        bytes = <BigEndian as SliceSerializable<f64>>::write(bytes, data.z);
        bytes
    }

    fn get_write_size(_: DVec3) -> usize {
        12
    }
}

slice_serializable! {
    #[derive(Default, Clone)]
    pub(super) struct DebugState {
        pub base_tick_velocity: DVec3 as DVec3Serializer,
        pub local_player_ai_step_velocity: DVec3 as DVec3Serializer,
        pub living_ai_step_velocity: DVec3 as DVec3Serializer,
        pub travel_velocity: DVec3 as DVec3Serializer,
        pub move_relative_velocity: DVec3 as DVec3Serializer,
        pub move_velocity: DVec3 as DVec3Serializer,
        pub after_travel_velocity: DVec3 as DVec3Serializer,
        pub move_inputs: DVec3 as DVec3Serializer,
        pub move_relative_speed: f32 as BigEndian,
        pub is_in_water: bool as Single,
        pub is_in_lava: bool as Single,
        pub is_swimming: bool as Single,
        pub is_sprinting: bool as Single,
        pub has_sprint_modifier_applied: bool as Single,
    } 
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ForcedPose {
    Standing = 0, // (0.6, 1.8)
    Crouching = 1, // (0.6, 1.5)
    FallFlyingSwimmingOrSpinAttack = 2, // (0.6, 0.6)
    None = 3
}

impl ForcedPose {
    pub fn from_pose(pose: Pose) -> Self {
        match pose {
            Pose::Standing => Self::Standing,
            Pose::FallFlying | Pose::Swimming | Pose::SpinAttack => Self::FallFlyingSwimmingOrSpinAttack,
            Pose::Crouching => Self::Crouching,
            _ => Self::None
        }
    }

    pub fn can_fit_when(self, pose: Pose) -> bool {
        self <= ForcedPose::from_pose(pose)
    }
}

#[derive(Clone)]
pub(super) struct KnownClientState {
    pub(super) debug_state: DebugState,
    pub(super) awaiting_absolute_teleport_count: u8,
    pub(super) pose: Pose,
    pub(super) forced_pose: ForcedPose,
    pub(super) velocity: UncertainVelocity,
    pub(super) living_flags: u8,
    pub(super) use_item: Item,
    pub(super) held_item_stack: ItemStack,
    pub(super) shared_flags: u8,
    pub(super) can_fly: bool,
    pub(super) is_flying_pre_tick: bool,
    pub(super) is_flying: bool,
    pub(super) flying_speed: f32,
    pub(super) sneaking_speed: f64,
    pub(super) applied_sprint_modifier_to_movement_speed: bool,
    pub(super) movement_speed: f64,
    pub(super) jump_strength: f64,
    pub(super) gravity: f64,
    pub(super) step_height: f64,
    pub(super) water_movement_efficiency: f64,
    pub(super) effects: EnumMap<MobEffect, Option<i32>>,
    pub(super) known_blocks: FxHashMap<IVec3, u16>
}
