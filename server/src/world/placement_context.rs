use minecraft_constants::{
    block::{Block, BlockProperties, NoSuchBlockError},
    block_parameter::{self, Axis3D, DirectionOrDown, Facing, Half},
    placement::PlacementContext,
};
use protocol::types::{BlockPosition, Direction};

use super::{World, WorldService};

pub struct ServerPlacementContext<'a, W: WorldService> {
    pub(crate) interacted_pos: BlockPosition,
    pub(crate) offset_pos: BlockPosition,
    pub(crate) click_offset: (f32, f32, f32),
    pub(crate) face: Direction,
    pub(crate) placer_yaw: f32,
    pub(crate) placer_pitch: f32,

    pub(crate) world: &'a mut World<W>,
    pub(crate) existing_block_id: Option<Option<u16>>,
    pub(crate) existing_block: Option<Option<&'a Block>>,
}

impl<'a, W: WorldService> ServerPlacementContext<'a, W> {
    fn get_block_id(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        self.world.get_block_i32(x, y, z)
    }

    fn get_offset_block_id(&mut self) -> Option<u16> {
        if let Some(existing_block_id) = self.existing_block_id {
            existing_block_id
        } else {
            let state_id =
                self.get_block_id(self.offset_pos.x, self.offset_pos.y as _, self.offset_pos.z);
            self.existing_block_id = Some(state_id);
            state_id
        }
    }

    fn get_block(&self, x: i32, y: i32, z: i32) -> Option<&Block> {
        self.get_block_id(x, y, z).and_then(|id| id.try_into().ok())
    }

    fn get_offset_block(&mut self) -> Option<&Block> {
        if let Some(existing_block) = self.existing_block {
            existing_block
        } else {
            let block = self.get_offset_block_id().and_then(|id| id.try_into().ok());
            self.existing_block = Some(block);
            block
        }
    }

    fn facing_opposite(facing: Facing) -> Facing {
        match facing {
            Facing::Down => Facing::Up,
            Facing::Up => Facing::Down,
            Facing::North => Facing::South,
            Facing::South => Facing::North,
            Facing::West => Facing::East,
            Facing::East => Facing::West,
        }
    }

    fn direction_opposite(facing: block_parameter::Direction) -> block_parameter::Direction {
        match facing {
            block_parameter::Direction::North => block_parameter::Direction::South,
            block_parameter::Direction::South => block_parameter::Direction::North,
            block_parameter::Direction::West => block_parameter::Direction::East,
            block_parameter::Direction::East => block_parameter::Direction::West,
        }
    }

    fn get_all_facing(&self) -> [Facing; 6] {
        let (pitch_sin, pitch_cos) = self.placer_pitch.to_radians().sin_cos();
        let (yaw_sin, yaw_cos) = (-self.placer_yaw).to_radians().sin_cos();

        let yaw_sin_abs = yaw_sin.abs();
        let pitch_sin_abs = pitch_sin.abs();
        let yaw_cos_abs = yaw_cos.abs();
        let scaled_yaw_sin_abs = yaw_sin_abs * pitch_cos;
        let scaled_yaw_cos_abs = yaw_cos_abs * pitch_cos;
        let east_west = if yaw_sin > 0.0 {
            Facing::East
        } else {
            Facing::West
        };
        let up_down = if pitch_sin < 0.0 {
            Facing::Up
        } else {
            Facing::Down
        };
        let south_north = if yaw_cos > 0.0 {
            Facing::South
        } else {
            Facing::North
        };
        if yaw_sin_abs > yaw_cos_abs {
            if pitch_sin_abs > scaled_yaw_sin_abs {
                [
                    up_down,
                    east_west,
                    south_north,
                    Self::facing_opposite(south_north),
                    Self::facing_opposite(east_west),
                    Self::facing_opposite(up_down),
                ]
            } else if scaled_yaw_cos_abs > pitch_sin_abs {
                [
                    east_west,
                    south_north,
                    up_down,
                    Self::facing_opposite(up_down),
                    Self::facing_opposite(south_north),
                    Self::facing_opposite(east_west),
                ]
            } else {
                [
                    east_west,
                    up_down,
                    south_north,
                    Self::facing_opposite(south_north),
                    Self::facing_opposite(up_down),
                    Self::facing_opposite(east_west),
                ]
            }
        } else {
            if pitch_sin_abs > scaled_yaw_cos_abs {
                [
                    up_down,
                    south_north,
                    east_west,
                    Self::facing_opposite(east_west),
                    Self::facing_opposite(south_north),
                    Self::facing_opposite(up_down),
                ]
            } else if scaled_yaw_sin_abs > pitch_sin_abs {
                [
                    south_north,
                    east_west,
                    up_down,
                    Self::facing_opposite(up_down),
                    Self::facing_opposite(east_west),
                    Self::facing_opposite(south_north),
                ]
            } else {
                [
                    south_north,
                    up_down,
                    east_west,
                    Self::facing_opposite(east_west),
                    Self::facing_opposite(up_down),
                    Self::facing_opposite(south_north),
                ]
            }
        }
    }
}

impl<'a, W: WorldService> PlacementContext for ServerPlacementContext<'a, W> {
    fn get_clicked_axis(&mut self) -> Axis3D {
        match self.face {
            Direction::Down => Axis3D::Y,
            Direction::Up => Axis3D::Y,
            Direction::North => Axis3D::Z,
            Direction::South => Axis3D::Z,
            Direction::West => Axis3D::X,
            Direction::East => Axis3D::X,
        }
    }

    fn get_clicked_half(&mut self) -> Half {
        match self.face {
            Direction::Down => Half::Top,
            Direction::Up => Half::Bottom,
            Direction::North => {
                if self.click_offset.1 <= 0.5 {
                    Half::Bottom
                } else {
                    Half::Top
                }
            }
            Direction::South => {
                if self.click_offset.1 <= 0.5 {
                    Half::Bottom
                } else {
                    Half::Top
                }
            }
            Direction::West => {
                if self.click_offset.1 <= 0.5 {
                    Half::Bottom
                } else {
                    Half::Top
                }
            }
            Direction::East => {
                if self.click_offset.1 <= 0.5 {
                    Half::Bottom
                } else {
                    Half::Top
                }
            }
        }
    }

    fn get_facing_big_dripleaf(&mut self) -> block_parameter::Direction {
        let below = self.get_block(
            self.offset_pos.x,
            self.offset_pos.y as i32 - 1,
            self.offset_pos.z,
        );
        if let Some(below) = below {
            match below {
                Block::BigDripleaf {
                    facing,
                    tilt: _,
                    waterlogged: _,
                }
                | Block::BigDripleafStem {
                    facing,
                    waterlogged: _,
                } => {
                    return *facing;
                }
                _ => (),
            }
        }
        self.get_facing_look_horizontal_opposite()
    }

    fn get_facing_clicked(&mut self) -> Facing {
        match self.face {
            Direction::Down => Facing::Down,
            Direction::Up => Facing::Up,
            Direction::North => Facing::North,
            Direction::South => Facing::South,
            Direction::West => Facing::West,
            Direction::East => Facing::East,
        }
    }

    fn get_facing_clicked_horizontal_opposite_else_down(&mut self) -> DirectionOrDown {
        match self.face {
            Direction::Down => DirectionOrDown::Down,
            Direction::Up => DirectionOrDown::Down,
            Direction::North => DirectionOrDown::South,
            Direction::South => DirectionOrDown::North,
            Direction::West => DirectionOrDown::East,
            Direction::East => DirectionOrDown::West,
        }
    }

    fn get_facing_look(&mut self) -> Facing {
        let (pitch_sin, pitch_cos) = self.placer_pitch.to_radians().sin_cos();
        let (yaw_sin, yaw_cos) = (-self.placer_yaw).to_radians().sin_cos();

        let yaw_sin_abs = yaw_sin.abs();
        let pitch_sin_abs = pitch_sin.abs();
        let yaw_cos_abs = yaw_cos.abs();
        if yaw_sin_abs > yaw_cos_abs {
            if pitch_sin_abs > yaw_sin_abs * pitch_cos {
                if pitch_sin < 0.0 {
                    Facing::Up
                } else {
                    Facing::Down
                }
            } else {
                if yaw_sin > 0.0 {
                    Facing::East
                } else {
                    Facing::West
                }
            }
        } else {
            if pitch_sin_abs > yaw_cos_abs * pitch_cos {
                if pitch_sin < 0.0 {
                    Facing::Up
                } else {
                    Facing::Down
                }
            } else {
                if yaw_cos > 0.0 {
                    Facing::South
                } else {
                    Facing::North
                }
            }
        }
    }

    fn get_facing_look_horizontal(&mut self) -> block_parameter::Direction {
        let (yaw_sin, yaw_cos) = (-self.placer_yaw).to_radians().sin_cos();

        if yaw_sin.abs() > yaw_cos.abs() {
            if yaw_sin > 0.0 {
                block_parameter::Direction::East
            } else {
                block_parameter::Direction::West
            }
        } else {
            if yaw_cos > 0.0 {
                block_parameter::Direction::South
            } else {
                block_parameter::Direction::North
            }
        }
    }

    fn get_facing_look_horizontal_nonreplacable_opposite(&mut self) -> block_parameter::Direction {
        // todo: check if it is nonreplacable
        // for now, this check has been omitted because its useful for building
        self.get_facing_look_horizontal_opposite()
    }

    fn get_facing_look_horizontal_opposite(&mut self) -> block_parameter::Direction {
        Self::direction_opposite(self.get_facing_look_horizontal())
    }

    fn get_facing_look_horizontal_plus_90(&mut self) -> block_parameter::Direction {
        match self.get_facing_look_horizontal() {
            block_parameter::Direction::North => block_parameter::Direction::East,
            block_parameter::Direction::South => block_parameter::Direction::West,
            block_parameter::Direction::West => block_parameter::Direction::North,
            block_parameter::Direction::East => block_parameter::Direction::South,
        }
    }

    fn get_facing_look_horizontal_survivable_opposite(&mut self) -> block_parameter::Direction {
        // todo: check if it is survivable
        // for now, this check has been omitted because its useful for building
        self.get_facing_look_horizontal_opposite()
    }

    fn get_facing_look_opposite(&mut self) -> Facing {
        Self::facing_opposite(self.get_facing_look())
    }

    fn get_fence_should_connect_east(&mut self) -> bool {
        let id = self.get_block_id(
            self.offset_pos.x + 1,
            self.offset_pos.y as i32,
            self.offset_pos.z,
        );
        if let Some(id) = id {
            let properties: Result<&BlockProperties, NoSuchBlockError> = id.try_into();
            if let Ok(properties) = properties {
                // todo: check if this is a fence
                return properties.is_west_face_sturdy
            }
        }
        false
    }

    fn get_fence_should_connect_north(&mut self) -> bool {
        let id = self.get_block_id(
            self.offset_pos.x,
            self.offset_pos.y as i32,
            self.offset_pos.z - 1,
        );
        if let Some(id) = id {
            let properties: Result<&BlockProperties, NoSuchBlockError> = id.try_into();
            if let Ok(properties) = properties {
                // todo: check if this is a fence
                return properties.is_south_face_sturdy
            }
        }
        false
    }

    fn get_fence_should_connect_south(&mut self) -> bool {
        let id = self.get_block_id(
            self.offset_pos.x,
            self.offset_pos.y as i32,
            self.offset_pos.z + 1,
        );
        if let Some(id) = id {
            let properties: Result<&BlockProperties, NoSuchBlockError> = id.try_into();
            if let Ok(properties) = properties {
                // todo: check if this is a fence
                return properties.is_north_face_sturdy
            }
        }
        false
    }

    fn get_fence_should_connect_west(&mut self) -> bool {
        let id = self.get_block_id(
            self.offset_pos.x - 1,
            self.offset_pos.y as i32,
            self.offset_pos.z,
        );
        if let Some(id) = id {
            let properties: Result<&BlockProperties, NoSuchBlockError> = id.try_into();
            if let Ok(properties) = properties {
                // todo: check if this is a fence
                return properties.is_east_face_sturdy
            }
        }
        false
    }

    fn get_hanging(&mut self) -> bool {
        todo!()
    }

    fn get_instrument_modifier_below(
        &mut self,
    ) -> minecraft_constants::block_parameter::Instrument {
        todo!()
    }

    fn get_iron_bars_should_connect_east(&mut self) -> bool {
        todo!()
    }

    fn get_iron_bars_should_connect_north(&mut self) -> bool {
        todo!()
    }

    fn get_iron_bars_should_connect_south(&mut self) -> bool {
        todo!()
    }

    fn get_iron_bars_should_connect_west(&mut self) -> bool {
        todo!()
    }

    fn get_leaves_distance(&mut self) -> u8 {
        todo!()
    }

    fn get_rail_shape(&mut self) -> minecraft_constants::block_parameter::RailShape {
        todo!()
    }

    fn get_rail_shape_straight(
        &mut self,
    ) -> minecraft_constants::block_parameter::StraightRailShape {
        todo!()
    }

    fn get_random_25(&mut self) -> u8 {
        0
    }

    fn get_rotation_16(&mut self) -> u8 {
        ((self.placer_yaw + 360.0) * 16.0 / 360.0 + 0.5).floor() as u8 & 15
    }

    fn get_rotation_16_flipped(&mut self) -> u8 {
        ((self.placer_yaw + 180.0) * 16.0 / 360.0 + 0.5).floor() as u8 & 15
    }

    fn get_scaffold_distance(&mut self) -> u8 {
        todo!()
    }

    fn get_scaffold_is_bottom(&mut self) -> bool {
        todo!()
    }

    fn get_stair_shape(&mut self) -> minecraft_constants::block_parameter::StairShape {
        todo!()
    }

    fn get_tripwire_should_connect_east(&mut self) -> bool {
        todo!()
    }

    fn get_tripwire_should_connect_north(&mut self) -> bool {
        todo!()
    }

    fn get_tripwire_should_connect_south(&mut self) -> bool {
        todo!()
    }

    fn get_tripwire_should_connect_west(&mut self) -> bool {
        todo!()
    }

    fn has_neighbor_signal(&mut self) -> bool {
        false
    }

    fn has_same_block_above(&mut self) -> bool {
        todo!()
    }

    fn has_same_block_below(&mut self) -> bool {
        todo!()
    }

    fn has_same_block_east(&mut self) -> bool {
        todo!()
    }

    fn has_same_block_north(&mut self) -> bool {
        todo!()
    }

    fn has_same_block_south(&mut self) -> bool {
        todo!()
    }

    fn has_same_block_west(&mut self) -> bool {
        todo!()
    }

    fn has_smoke_source_below(&mut self) -> bool {
        todo!()
    }

    fn has_snow_above(&mut self) -> bool {
        match self.get_block(
            self.offset_pos.x,
            self.offset_pos.y as i32 + 1,
            self.offset_pos.z,
        ) {
            Some(Block::SnowBlock) | Some(Block::Snow { layers: _ }) | Some(Block::PowderSnow) => {
                return true
            }
            _ => return false,
        }
    }

    fn is_in_water(&mut self) -> bool {
        const WATER_STATE_ID: u16 = (&Block::Water { level: 0 }).to_id();
        if let Some(state_id) = self.get_offset_block_id() {
            state_id == WATER_STATE_ID
        } else {
            false
        }
    }

    fn is_not_in_water(&mut self) -> bool {
        !self.is_in_water()
    }

    fn is_repeater_locked(&mut self) -> bool {
        false
    }
}
