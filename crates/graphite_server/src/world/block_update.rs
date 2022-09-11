use graphite_mc_constants::{block::{Block, BlockAttributes, NoSuchBlockError, self}, block_parameter::{RailShape, StraightRailShape, self, Half, StairShape}, tags::block::BlockTags};


use super::{World, WorldService};

pub fn update<W: WorldService>(block_state_id: u16, block: &mut Block, x: i32, y: i32, z: i32, world: &mut World<W>) -> bool {
    match block {
        Block::GrassBlock { snowy } |
        Block::Podzol { snowy } |
        Block::Mycelium { snowy } => {
            let above_id = world.get_block_i32(x, y + 1, z);
            let above_block = above_id.and_then(|id| <&Block>::try_from(id).ok());
            let is_snowy = causes_snowy(above_block);

            if is_snowy != *snowy {
                *snowy = is_snowy;
                return true;
            }

            return false; // unchanged
        },
        
        Block::Rail { shape, waterlogged: _ } => {
            if let Some(new_shape) = get_rail_shape(x, y, z, world) {
                if *shape != new_shape {
                    *shape = new_shape;
                    return true;
                }
            }
            return false;
        },
        Block::PoweredRail { powered: _, shape, waterlogged: _ } |
        Block::DetectorRail { powered: _, shape, waterlogged: _ } |
        Block::ActivatorRail { powered: _, shape, waterlogged: _ } => {
            if let Some(new_shape) = get_straight_rail_shape(x, y, z, world) {
                if *shape != new_shape {
                    *shape = new_shape;
                    return true;
                }
            }
            return false;
        },
        
        Block::OakStairs { facing, half, shape, waterlogged: _ } |
        Block::CobblestoneStairs { facing, half, shape, waterlogged: _ } |
        Block::BrickStairs { facing, half, shape, waterlogged: _ } |
        Block::StoneBrickStairs { facing, half, shape, waterlogged: _ } |
        Block::MudBrickStairs { facing, half, shape, waterlogged: _ } |
        Block::SpruceStairs { facing, half, shape, waterlogged: _ } |
        Block::BirchStairs { facing, half, shape, waterlogged: _ } |
        Block::JungleStairs { facing, half, shape, waterlogged: _ } |
        Block::NetherBrickStairs { facing, half, shape, waterlogged: _ } |
        Block::SandstoneStairs { facing, half, shape, waterlogged: _ } |
        Block::QuartzStairs { facing, half, shape, waterlogged: _ } |
        Block::AcaciaStairs { facing, half, shape, waterlogged: _ } |
        Block::DarkOakStairs { facing, half, shape, waterlogged: _ } |
        Block::MangroveStairs { facing, half, shape, waterlogged: _ } |
        Block::PrismarineStairs { facing, half, shape, waterlogged: _ } |
        Block::PrismarineBrickStairs { facing, half, shape, waterlogged: _ } |
        Block::DarkPrismarineStairs { facing, half, shape, waterlogged: _ } |
        Block::RedSandstoneStairs { facing, half, shape, waterlogged: _ } |
        Block::PurpurStairs { facing, half, shape, waterlogged: _ } |
        Block::PolishedGraniteStairs { facing, half, shape, waterlogged: _ } |
        Block::SmoothRedSandstoneStairs { facing, half, shape, waterlogged: _ } |
        Block::MossyStoneBrickStairs { facing, half, shape, waterlogged: _ } |
        Block::PolishedDioriteStairs { facing, half, shape, waterlogged: _ } |
        Block::MossyCobblestoneStairs { facing, half, shape, waterlogged: _ } |
        Block::EndStoneBrickStairs { facing, half, shape, waterlogged: _ } |
        Block::StoneStairs { facing, half, shape, waterlogged: _ } |
        Block::SmoothSandstoneStairs { facing, half, shape, waterlogged: _ } |
        Block::SmoothQuartzStairs { facing, half, shape, waterlogged: _ } |
        Block::GraniteStairs { facing, half, shape, waterlogged: _ } |
        Block::AndesiteStairs { facing, half, shape, waterlogged: _ } |
        Block::RedNetherBrickStairs { facing, half, shape, waterlogged: _ } |
        Block::PolishedAndesiteStairs { facing, half, shape, waterlogged: _ } |
        Block::DioriteStairs { facing, half, shape, waterlogged: _ } |
        Block::CrimsonStairs { facing, half, shape, waterlogged: _ } |
        Block::WarpedStairs { facing, half, shape, waterlogged: _ } |
        Block::BlackstoneStairs { facing, half, shape, waterlogged: _ } |
        Block::PolishedBlackstoneBrickStairs { facing, half, shape, waterlogged: _ } |
        Block::PolishedBlackstoneStairs { facing, half, shape, waterlogged: _ } |
        Block::OxidizedCutCopperStairs { facing, half, shape, waterlogged: _ } |
        Block::WeatheredCutCopperStairs { facing, half, shape, waterlogged: _ } |
        Block::ExposedCutCopperStairs { facing, half, shape, waterlogged: _ } |
        Block::CutCopperStairs { facing, half, shape, waterlogged: _ } |
        Block::WaxedOxidizedCutCopperStairs { facing, half, shape, waterlogged: _ } |
        Block::WaxedWeatheredCutCopperStairs { facing, half, shape, waterlogged: _ } |
        Block::WaxedExposedCutCopperStairs { facing, half, shape, waterlogged: _ } |
        Block::WaxedCutCopperStairs { facing, half, shape, waterlogged: _ } |
        Block::CobbledDeepslateStairs { facing, half, shape, waterlogged: _ } |
        Block::PolishedDeepslateStairs { facing, half, shape, waterlogged: _ } |
        Block::DeepslateTileStairs { facing, half, shape, waterlogged: _ } |
        Block::DeepslateBrickStairs { facing, half, shape, waterlogged: _ } => {
            let new_shape = get_stair_shape(x, y, z, *facing, *half, world);
            if *shape != new_shape {
                *shape = new_shape;
                return true;
            }
            return false;
        }
        
        Block::RedstoneWire { east: _, north: _, power: _, south: _, west: _ } => todo!(),
        
        Block::OakFence { east, north, south, waterlogged: _, west } |
        Block::NetherBrickFence { east, north, south, waterlogged: _, west } |
        Block::SpruceFence { east, north, south, waterlogged: _, west } |
        Block::BirchFence { east, north, south, waterlogged: _, west } |
        Block::JungleFence { east, north, south, waterlogged: _, west } |
        Block::AcaciaFence { east, north, south, waterlogged: _, west } |
        Block::DarkOakFence { east, north, south, waterlogged: _, west } |
        Block::MangroveFence { east, north, south, waterlogged: _, west } |
        Block::CrimsonFence { east, north, south, waterlogged: _, west } |
        Block::WarpedFence { east, north, south, waterlogged: _, west } => {
            let mut changed = false;

            let new_east = should_fence_connect(x, y, z, block_parameter::Direction::East, world);
            if *east != new_east {
                *east = new_east;
                changed = true;
            }

            let new_north = should_fence_connect(x, y, z, block_parameter::Direction::North, world);
            if *north != new_north {
                *north = new_north;
                changed = true;
            }

            let new_south = should_fence_connect(x, y, z, block_parameter::Direction::South, world);
            if *south != new_south {
                *south = new_south;
                changed = true;
            }

            let new_west = should_fence_connect(x, y, z, block_parameter::Direction::West, world);
            if *west != new_west {
                *west = new_west;
                changed = true;
            }

            return changed;
        }
        
        Block::IronBars { east, north, south, waterlogged: _, west } |
        Block::GlassPane { east, north, south, waterlogged: _, west } |
        Block::WhiteStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::OrangeStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::MagentaStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::LightBlueStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::YellowStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::LimeStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::PinkStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::GrayStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::LightGrayStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::CyanStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::PurpleStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::BlueStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::BrownStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::GreenStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::RedStainedGlassPane { east, north, south, waterlogged: _, west } |
        Block::BlackStainedGlassPane { east, north, south, waterlogged: _, west } => {
            let mut changed = false;

            let new_east = should_iron_bars_connect(x, y, z, block_parameter::Direction::East, world);
            if *east != new_east {
                *east = new_east;
                changed = true;
            }

            let new_north = should_iron_bars_connect(x, y, z, block_parameter::Direction::North, world);
            if *north != new_north {
                *north = new_north;
                changed = true;
            }

            let new_south = should_iron_bars_connect(x, y, z, block_parameter::Direction::South, world);
            if *south != new_south {
                *south = new_south;
                changed = true;
            }

            let new_west = should_iron_bars_connect(x, y, z, block_parameter::Direction::West, world);
            if *west != new_west {
                *west = new_west;
                changed = true;
            }

            return changed;
        }
        
        Block::BrownMushroomBlock { down, east, north, south, up, west } |
        Block::RedMushroomBlock { down, east, north, south, up, west } |
        Block::MushroomStem { down, east, north, south, up, west } => {
            let mut changed = false;
            let item = block::state_to_item(block_state_id).expect("valid block");

            if *down {
                let has_same_block_down = world.get_block_i32(x, y - 1, z)
                    .and_then(|id| block::state_to_item(id).ok())
                    .map(|other_item| item == other_item).unwrap_or(false);
                if has_same_block_down {
                    *down = false;
                    changed = true;
                }
            }

            if *east {
                let has_same_block_east = world.get_block_i32(x + 1, y, z)
                    .and_then(|id| block::state_to_item(id).ok())
                    .map(|other_item| item == other_item).unwrap_or(false);
                if has_same_block_east {
                    *east = false;
                    changed = true;
                }
            }

            if *north {
                let has_same_block_north = world.get_block_i32(x, y, z - 1)
                    .and_then(|id| block::state_to_item(id).ok())
                    .map(|other_item| item == other_item).unwrap_or(false);
                if has_same_block_north {
                    *north = false;
                    changed = true;
                }
            }

            if *south {
                let has_same_block_south = world.get_block_i32(x, y, z + 1)
                    .and_then(|id| block::state_to_item(id).ok())
                    .map(|other_item| item == other_item).unwrap_or(false);
                if has_same_block_south {
                    *south = false;
                    changed = true;
                }
            }

            if *up {
                let has_same_block_up = world.get_block_i32(x, y + 1, z)
                    .and_then(|id| block::state_to_item(id).ok())
                    .map(|other_item| item == other_item).unwrap_or(false);
                if has_same_block_up {
                    *up = false;
                    changed = true;
                }
            }

            if *west {
                let has_same_block_west = world.get_block_i32(x - 1, y, z)
                    .and_then(|id| block::state_to_item(id).ok())
                    .map(|other_item| item == other_item).unwrap_or(false);
                if has_same_block_west {
                    *west = false;
                    changed = true;
                }
            }

            return changed;
        }
        
        Block::CobblestoneWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::MossyCobblestoneWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::BrickWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::PrismarineWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::RedSandstoneWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::MossyStoneBrickWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::GraniteWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::StoneBrickWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::MudBrickWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::NetherBrickWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::AndesiteWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::RedNetherBrickWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::SandstoneWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::EndStoneBrickWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::DioriteWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::BlackstoneWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::PolishedBlackstoneBrickWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::PolishedBlackstoneWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::CobbledDeepslateWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::PolishedDeepslateWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::DeepslateTileWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),
        Block::DeepslateBrickWall { east: _, north: _, south: _, up: _, waterlogged: _, west: _ } => todo!(),

        Block::Tripwire { attached: _, disarmed: _, east: _, north: _, powered: _, south: _, west: _ } => todo!(),
        
        Block::PointedDripstone { thickness: _, vertical_direction: _, waterlogged: _ } => todo!(),

        Block::CaveVines { age: _, berries: _ } => todo!(),
        Block::CaveVinesPlant { berries: _ } => todo!(),
        
        Block::BigDripleaf { facing: _, tilt: _, waterlogged: _ } => todo!(),
        Block::BigDripleafStem { facing: _, waterlogged: _ } => todo!(),

        Block::Scaffolding { bottom: _, distance: _, waterlogged: _ } => todo!(),

        _ => return false,

        // These blocks *could* have updates, but I have chosen not to
        // implement them because creative building is easier without them

        // Noteblock could update its instrument
        
        // Block::NoteBlock { instrument, note, powered } => todo!(),

        // These blocks are composed of multiple parts. They could be destroyed
        // if a single part is destroyed

        // Block::WhiteBed { facing, occupied, part } => todo!(),
        // Block::OrangeBed { facing, occupied, part } => todo!(),
        // Block::MagentaBed { facing, occupied, part } => todo!(),
        // Block::LightBlueBed { facing, occupied, part } => todo!(),
        // Block::YellowBed { facing, occupied, part } => todo!(),
        // Block::LimeBed { facing, occupied, part } => todo!(),
        // Block::PinkBed { facing, occupied, part } => todo!(),
        // Block::GrayBed { facing, occupied, part } => todo!(),
        // Block::LightGrayBed { facing, occupied, part } => todo!(),
        // Block::CyanBed { facing, occupied, part } => todo!(),
        // Block::PurpleBed { facing, occupied, part } => todo!(),
        // Block::BlueBed { facing, occupied, part } => todo!(),
        // Block::BrownBed { facing, occupied, part } => todo!(),
        // Block::GreenBed { facing, occupied, part } => todo!(),
        // Block::RedBed { facing, occupied, part } => todo!(),
        // Block::BlackBed { facing, occupied, part } => todo!(),
        // Block::TallSeagrass { half } => todo!(),
        // Block::SmallDripleaf { facing, half, waterlogged } => todo!(),
        // Block::Sunflower { half } => todo!(),
        // Block::Lilac { half } => todo!(),
        // Block::RoseBush { half } => todo!(),
        // Block::Peony { half } => todo!(),
        // Block::TallGrass { half } => todo!(),
        // Block::LargeFern { half } => todo!(),
        // Block::Chest { facing, block_type, waterlogged } => todo!(),
        // Block::TrappedChest { facing, block_type, waterlogged } => todo!(),
        // Block::OakDoor { facing, half, hinge, open, powered } => todo!(),
        // Block::IronDoor { facing, half, hinge, open, powered } => todo!(),
        // Block::SpruceDoor { facing, half, hinge, open, powered } => todo!(),
        // Block::BirchDoor { facing, half, hinge, open, powered } => todo!(),
        // Block::JungleDoor { facing, half, hinge, open, powered } => todo!(),
        // Block::AcaciaDoor { facing, half, hinge, open, powered } => todo!(),
        // Block::DarkOakDoor { facing, half, hinge, open, powered } => todo!(),
        // Block::MangroveDoor { facing, half, hinge, open, powered } => todo!(),
        // Block::CrimsonDoor { facing, half, hinge, open, powered } => todo!(),
        // Block::WarpedDoor { facing, half, hinge, open, powered } => todo!(),
    }
}

pub(crate) fn causes_snowy(block: Option<&Block>) -> bool {
    return matches!(block, Some(Block::SnowBlock) | Some(Block::Snow { layers: _ }) | Some(Block::PowderSnow));
}

fn is_rail<W: WorldService>(x: i32, y: i32, z: i32, world: &World<W>) -> bool {
    let block = world.get_block_i32(x, y, z);
    if let Some(block) = block {
        return BlockTags::Rails.contains(block);
    } else {
        return false;
    }
}

fn get_stair_properties<W: WorldService>(x: i32, y: i32, z: i32, world: &World<W>) -> Option<(block_parameter::Direction, Half, StairShape, bool)> {
    let block = world.get_block_i32(x, y, z).and_then(|id| <&Block>::try_from(id).ok());
    if let Some(block) = block {
        match block {
            Block::OakStairs { facing, half, shape, waterlogged } |
            Block::CobblestoneStairs { facing, half, shape, waterlogged } |
            Block::BrickStairs { facing, half, shape, waterlogged } |
            Block::StoneBrickStairs { facing, half, shape, waterlogged } |
            Block::MudBrickStairs { facing, half, shape, waterlogged } |
            Block::NetherBrickStairs { facing, half, shape, waterlogged } |
            Block::SandstoneStairs { facing, half, shape, waterlogged } |
            Block::SpruceStairs { facing, half, shape, waterlogged } |
            Block::BirchStairs { facing, half, shape, waterlogged } |
            Block::JungleStairs { facing, half, shape, waterlogged } |
            Block::QuartzStairs { facing, half, shape, waterlogged } |
            Block::AcaciaStairs { facing, half, shape, waterlogged } |
            Block::DarkOakStairs { facing, half, shape, waterlogged } |
            Block::MangroveStairs { facing, half, shape, waterlogged } |
            Block::PrismarineStairs { facing, half, shape, waterlogged } |
            Block::PrismarineBrickStairs { facing, half, shape, waterlogged } |
            Block::DarkPrismarineStairs { facing, half, shape, waterlogged } |
            Block::RedSandstoneStairs { facing, half, shape, waterlogged } |
            Block::PurpurStairs { facing, half, shape, waterlogged } |
            Block::PolishedGraniteStairs { facing, half, shape, waterlogged } |
            Block::SmoothRedSandstoneStairs { facing, half, shape, waterlogged } |
            Block::MossyStoneBrickStairs { facing, half, shape, waterlogged } |
            Block::PolishedDioriteStairs { facing, half, shape, waterlogged } |
            Block::MossyCobblestoneStairs { facing, half, shape, waterlogged } |
            Block::EndStoneBrickStairs { facing, half, shape, waterlogged } |
            Block::StoneStairs { facing, half, shape, waterlogged } |
            Block::SmoothSandstoneStairs { facing, half, shape, waterlogged } |
            Block::SmoothQuartzStairs { facing, half, shape, waterlogged } |
            Block::GraniteStairs { facing, half, shape, waterlogged } |
            Block::AndesiteStairs { facing, half, shape, waterlogged } |
            Block::RedNetherBrickStairs { facing, half, shape, waterlogged } |
            Block::PolishedAndesiteStairs { facing, half, shape, waterlogged } |
            Block::DioriteStairs { facing, half, shape, waterlogged } |
            Block::CrimsonStairs { facing, half, shape, waterlogged } |
            Block::WarpedStairs { facing, half, shape, waterlogged } |
            Block::BlackstoneStairs { facing, half, shape, waterlogged } |
            Block::PolishedBlackstoneBrickStairs { facing, half, shape, waterlogged } |
            Block::PolishedBlackstoneStairs { facing, half, shape, waterlogged } |
            Block::OxidizedCutCopperStairs { facing, half, shape, waterlogged } |
            Block::WeatheredCutCopperStairs { facing, half, shape, waterlogged } |
            Block::ExposedCutCopperStairs { facing, half, shape, waterlogged } |
            Block::CutCopperStairs { facing, half, shape, waterlogged } |
            Block::WaxedOxidizedCutCopperStairs { facing, half, shape, waterlogged } |
            Block::WaxedWeatheredCutCopperStairs { facing, half, shape, waterlogged } |
            Block::WaxedExposedCutCopperStairs { facing, half, shape, waterlogged } |
            Block::WaxedCutCopperStairs { facing, half, shape, waterlogged } |
            Block::CobbledDeepslateStairs { facing, half, shape, waterlogged } |
            Block::PolishedDeepslateStairs { facing, half, shape, waterlogged } |
            Block::DeepslateTileStairs { facing, half, shape, waterlogged } |
            Block::DeepslateBrickStairs { facing, half, shape, waterlogged } => {
                Some((*facing, *half, *shape, *waterlogged))
            }
            _ => None
        }
    } else {
        return None;
    }

    
}

pub(crate) fn get_stair_shape<W: WorldService>(x: i32, y: i32, z: i32, direction: block_parameter::Direction, half: Half,
        world: &World<W>) -> StairShape {
    
    let behind = match direction {
        block_parameter::Direction::North => (x, y, z - 1),
        block_parameter::Direction::South => (x, y, z + 1),
        block_parameter::Direction::West => (x - 1, y, z),
        block_parameter::Direction::East => (x + 1, y, z),
    };

    let stair_properties = get_stair_properties(behind.0, behind.1, behind.2, world);
    if let Some((r_direction, r_half, r_shape, _)) = stair_properties {
        if half == r_half {
            let direction_i = match direction {
                block_parameter::Direction::North => 0,
                block_parameter::Direction::East => 1,
                block_parameter::Direction::South => 2,
                block_parameter::Direction::West => 3,
            };
            let r_direction_i = match r_direction {
                block_parameter::Direction::North => 0,
                block_parameter::Direction::East => 1,
                block_parameter::Direction::South => 2,
                block_parameter::Direction::West => 3,
            };

            let direction_difference = direction_i - r_direction_i;
            if (direction_difference == 1 || direction_difference == -3) && r_shape != StairShape::InnerLeft && r_shape != StairShape::OuterRight {
                let right = match direction {
                    block_parameter::Direction::North => (x + 1, y, z),
                    block_parameter::Direction::South => (x - 1, y, z),
                    block_parameter::Direction::West => (x, y, z - 1),
                    block_parameter::Direction::East => (x, y, z + 1),
                };
                if let Some((o_direction, o_half, _, _)) = get_stair_properties(right.0, right.1, right.2, world) {
                    if o_direction != direction || o_half != half {
                        return StairShape::OuterLeft;
                    }
                } else {
                    return StairShape::OuterLeft;
                }
                
            } else if (direction_difference == -1 || direction_difference == 3) && r_shape != StairShape::InnerRight && r_shape != StairShape::OuterLeft {
                let left = match direction {
                    block_parameter::Direction::North => (x - 1, y, z),
                    block_parameter::Direction::South => (x + 1, y, z),
                    block_parameter::Direction::West => (x, y, z + 1),
                    block_parameter::Direction::East => (x, y, z - 1),
                };
                if let Some((o_direction, o_half, _, _)) = get_stair_properties(left.0, left.1, left.2, world) {
                    if o_direction != direction || o_half != half {
                        return StairShape::OuterRight;
                    }
                } else {
                    return StairShape::OuterRight;
                }
            }
        }
    }

    let front = match direction {
        block_parameter::Direction::North => (x, y, z + 1),
        block_parameter::Direction::South => (x, y, z - 1),
        block_parameter::Direction::West => (x + 1, y, z),
        block_parameter::Direction::East => (x - 1, y, z),
    };
    let stair_properties = get_stair_properties(front.0, front.1, front.2, world);
    if let Some((r_direction, r_half, r_shape, _)) = stair_properties {
        if half == r_half {
            let direction_i = match direction {
                block_parameter::Direction::North => 0,
                block_parameter::Direction::East => 1,
                block_parameter::Direction::South => 2,
                block_parameter::Direction::West => 3,
            };
            let r_direction_i = match r_direction {
                block_parameter::Direction::North => 0,
                block_parameter::Direction::East => 1,
                block_parameter::Direction::South => 2,
                block_parameter::Direction::West => 3,
            };

            let direction_difference = direction_i - r_direction_i;
            if (direction_difference == 1 || direction_difference == -3) && r_shape != StairShape::InnerRight && r_shape != StairShape::OuterLeft {
                let left = match direction {
                    block_parameter::Direction::North => (x - 1, y, z),
                    block_parameter::Direction::South => (x + 1, y, z),
                    block_parameter::Direction::West => (x, y, z + 1),
                    block_parameter::Direction::East => (x, y, z - 1),
                };
                if let Some((o_direction, o_half, _, _)) = get_stair_properties(left.0, left.1, left.2, world) {
                    if o_direction != direction || o_half != half {
                        return StairShape::InnerLeft;
                    }
                } else {
                    return StairShape::InnerLeft;
                }
            } else if (direction_difference == -1 || direction_difference == 3)  && r_shape != StairShape::InnerLeft && r_shape != StairShape::OuterRight {
                let right = match direction {
                    block_parameter::Direction::North => (x + 1, y, z),
                    block_parameter::Direction::South => (x - 1, y, z),
                    block_parameter::Direction::West => (x, y, z - 1),
                    block_parameter::Direction::East => (x, y, z + 1),
                };
                if let Some((o_direction, o_half, _, _)) = get_stair_properties(right.0, right.1, right.2, world) {
                    if o_direction != direction || o_half != half {
                        return StairShape::InnerRight;
                    }
                } else {
                    return StairShape::InnerRight;
                }
            }
        }
    }

    StairShape::Straight
}

pub(crate) fn should_fence_connect<W: WorldService>(x: i32, y: i32, z: i32, direction: block_parameter::Direction, world: &World<W>) -> bool {
    let id = match direction {
        block_parameter::Direction::North => world.get_block_i32(x, y, z - 1),
        block_parameter::Direction::South => world.get_block_i32(x, y, z + 1),
        block_parameter::Direction::West => world.get_block_i32(x - 1, y, z),
        block_parameter::Direction::East => world.get_block_i32(x + 1, y, z),
    };
    if let Some(id) = id {
        if BlockTags::Fences.contains(id) {
            return true;
        }

        let properties: Result<&BlockAttributes, NoSuchBlockError> = id.try_into();
        if let Ok(properties) = properties {
            match direction {
                block_parameter::Direction::North => return properties.is_south_face_sturdy,
                block_parameter::Direction::South => return properties.is_north_face_sturdy,
                block_parameter::Direction::West => return properties.is_east_face_sturdy,
                block_parameter::Direction::East => return properties.is_west_face_sturdy,
            }
            
        }
    }
    false
}

pub(crate) fn should_iron_bars_connect<W: WorldService>(x: i32, y: i32, z: i32, direction: block_parameter::Direction, world: &World<W>) -> bool {
    let id = match direction {
        block_parameter::Direction::North => world.get_block_i32(x, y, z - 1),
        block_parameter::Direction::South => world.get_block_i32(x, y, z + 1),
        block_parameter::Direction::West => world.get_block_i32(x - 1, y, z),
        block_parameter::Direction::East => world.get_block_i32(x + 1, y, z),
    };
    if let Some(id) = id {
        if BlockTags::Walls.contains(id) {
            return true;
        }

        let properties: Result<&BlockAttributes, NoSuchBlockError> = id.try_into();
        if let Ok(properties) = properties {
            if properties.is_west_face_sturdy {
                if match direction {
                    block_parameter::Direction::North => properties.is_south_face_sturdy,
                    block_parameter::Direction::South => properties.is_north_face_sturdy,
                    block_parameter::Direction::West => properties.is_east_face_sturdy,
                    block_parameter::Direction::East => properties.is_west_face_sturdy,
                } {
                    return true;
                }
            }
        }
        
        if let Ok(block) = <&Block>::try_from(id) {
            match block {
                Block::IronBars { east: _, north: _, south: _, waterlogged: _, west: _ } |
                Block::GlassPane { east: _, north: _, south: _, waterlogged: _, west: _ } |
                Block::WhiteStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::OrangeStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::MagentaStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::LightBlueStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::YellowStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::LimeStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::PinkStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::GrayStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::LightGrayStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::CyanStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::PurpleStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::BlueStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::BrownStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::GreenStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::RedStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} |
                Block::BlackStainedGlassPane { east: _, north: _, south: _, waterlogged: _, west: _} => {
                    return true
                }
                _ => return false
            }
        }
    }
    false
}

pub(crate) fn get_rail_shape<W: WorldService>(x: i32, y: i32, z: i32, world: &World<W>) -> Option<RailShape> {
    let north = is_rail(x, y, z - 1, world) || is_rail(x, y - 1, z - 1, world);
    let east = is_rail(x + 1, y, z, world) || is_rail(x + 1, y - 1, z, world);
    let south = is_rail(x, y, z + 1, world) || is_rail(x, y - 1, z + 1, world);
    let west = is_rail(x - 1, y, z, world) || is_rail(x - 1, y - 1, z, world);
    
    if south {
        if east {
            return Some(RailShape::SouthEast);
        } else if west {
            return Some(RailShape::SouthWest);
        }

        let ascending_north = is_rail(x, y + 1, z - 1, world);
        if ascending_north {
            return Some(RailShape::AscendingNorth);
        } else {
            return Some(RailShape::NorthSouth);
        }
    } else if north {
        if east {
            return Some(RailShape::NorthEast);
        } else if west {
            return Some(RailShape::NorthWest);
        }

        let ascending_south = is_rail(x, y + 1, z + 1, world);
        if ascending_south {
            return Some(RailShape::AscendingSouth);
        } else {
            return Some(RailShape::NorthSouth);
        }
    } else if west {
        let ascending_east = is_rail(x + 1, y + 1, z, world);
        if ascending_east {
            return Some(RailShape::AscendingEast);
        } else {
            return Some(RailShape::EastWest);
        }
    } else if east {
        let ascending_west = is_rail(x - 1, y + 1, z, world);
        if ascending_west {
            return Some(RailShape::AscendingWest);
        } else {
            return Some(RailShape::EastWest);
        }
    } else {
        if is_rail(x, y + 1, z + 1, world) {
            return Some(RailShape::AscendingSouth);
        } else if is_rail(x, y + 1, z - 1, world) {
            return Some(RailShape::AscendingNorth);
        } else if is_rail(x - 1, y + 1, z, world) {
            return Some(RailShape::AscendingWest);
        } else if is_rail(x + 1, y + 1, z, world) {
            return Some(RailShape::AscendingEast);
        } else {
            return None;
        }
    }
}


pub(crate) fn get_straight_rail_shape<W: WorldService>(x: i32, y: i32, z: i32, world: &World<W>) -> Option<StraightRailShape> {
    let north = is_rail(x, y, z - 1, world) || is_rail(x, y - 1, z - 1, world);
    let east = is_rail(x + 1, y, z, world) || is_rail(x + 1, y - 1, z, world);
    let south = is_rail(x, y, z + 1, world) || is_rail(x, y - 1, z + 1, world);
    let west = is_rail(x - 1, y, z, world) || is_rail(x - 1, y - 1, z, world);
    
    if south {
        let ascending_north = is_rail(x, y + 1, z - 1, world);
        if ascending_north {
            return Some(StraightRailShape::AscendingNorth);
        } else {
            return Some(StraightRailShape::NorthSouth);
        }
    } else if north {
        let ascending_south = is_rail(x, y + 1, z + 1, world);
        if ascending_south {
            return Some(StraightRailShape::AscendingSouth);
        } else {
            return Some(StraightRailShape::NorthSouth);
        }
    } else if west {
        let ascending_east = is_rail(x + 1, y + 1, z, world);
        if ascending_east {
            return Some(StraightRailShape::AscendingEast);
        } else {
            return Some(StraightRailShape::EastWest);
        }
    } else if east {
        let ascending_west = is_rail(x - 1, y + 1, z, world);
        if ascending_west {
            return Some(StraightRailShape::AscendingWest);
        } else {
            return Some(StraightRailShape::EastWest);
        }
    } else {
        if is_rail(x, y + 1, z + 1, world) {
            return Some(StraightRailShape::AscendingSouth);
        } else if is_rail(x, y + 1, z - 1, world) {
            return Some(StraightRailShape::AscendingNorth);
        } else if is_rail(x - 1, y + 1, z, world) {
            return Some(StraightRailShape::AscendingWest);
        } else if is_rail(x + 1, y + 1, z, world) {
            return Some(StraightRailShape::AscendingEast);
        } else {
            return None;
        }
    }
}