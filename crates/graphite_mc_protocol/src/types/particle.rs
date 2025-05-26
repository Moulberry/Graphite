use graphite_binary::slice_serialization::*;
use graphite_mc_constants::{builtin, particle::{Particle, PositionSource}};

use super::BlockPosition;

pub(crate) struct SerializedParticle;

// todo: do this but for particle!
impl <'r, 'd: 'r> SliceSerializable<'r, 'd, Particle> for SerializedParticle {
    type CopyType = &'r Particle;

    #[inline(always)]
    fn as_copy_type(t: &'r Particle) -> Self::CopyType {
        t
    }

    fn read(_: &mut &'d [u8]) -> anyhow::Result<Particle> {
        todo!("reading particle unimplemented")
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        let id = data.get_id();
        bytes = <VarInt as SliceSerializable<i32>>::write(bytes, id);

        match data {
            Particle::AngryVillager => {},
            Particle::Block { block_state } => {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, *block_state as i32);
            },
            Particle::BlockMarker { block_state } => {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, *block_state as i32);
            },
            Particle::Bubble => {},
            Particle::Cloud => {},
            Particle::Crit => {},
            Particle::DamageIndicator => {},
            Particle::DragonBreath => {},
            Particle::DrippingLava => {},
            Particle::FallingLava => {},
            Particle::LandingLava => {},
            Particle::DrippingWater => {},
            Particle::FallingWater => {},
            Particle::Dust { color, scale } => {
                bytes = <BigEndian as SliceSerializable<i32>>::write(bytes, *color);
                bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, *scale);
            },
            Particle::DustColorTransition { from_color, to_color, scale } => {
                bytes = <BigEndian as SliceSerializable<i32>>::write(bytes, *from_color);
                bytes = <BigEndian as SliceSerializable<i32>>::write(bytes, *to_color);
                bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, *scale);
            },
            Particle::Effect => {},
            Particle::ElderGuardian => {},
            Particle::EnchantedHit => {},
            Particle::Enchant => {},
            Particle::EndRod => {},
            Particle::EntityEffect { color } => {
                bytes = <BigEndian as SliceSerializable<i32>>::write(bytes, *color);
            },
            Particle::ExplosionEmitter => {},
            Particle::Explosion => {},
            Particle::Gust => {},
            Particle::SmallGust => {},
            Particle::GustEmitterLarge => {},
            Particle::GustEmitterSmall => {},
            Particle::SonicBoom => {},
            Particle::FallingDust { block_state } => {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, *block_state as i32);
            },
            Particle::Firework => {},
            Particle::Fishing => {},
            Particle::Flame => {},
            Particle::Infested => {},
            Particle::CherryLeaves => {},
            Particle::SculkSoul => {},
            Particle::SculkCharge { roll } => {
                bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, *roll);
            },
            Particle::SculkChargePop => {},
            Particle::SoulFireFlame => {},
            Particle::Soul => {},
            Particle::Flash => {},
            Particle::HappyVillager => {},
            Particle::Composter => {},
            Particle::Heart => {},
            Particle::InstantEffect => {},
            Particle::Item { encoded_item } => {
                bytes = WriteOnlyBlob::write(bytes, encoded_item);
            },
            Particle::Vibration { destination, arrival_in_ticks } => {
                match destination {
                    PositionSource::Block { x, y, z } => {
                        bytes = <Single as SliceSerializable<u8>>::write(bytes, builtin::PositionSourceType::Block as u8);
                        bytes = BlockPosition::write(bytes, BlockPosition::new(*x, *y, *z));

                    },
                    PositionSource::Entity { id, y_offset } => {
                        bytes = <Single as SliceSerializable<u8>>::write(bytes, builtin::PositionSourceType::Entity as u8);
                        bytes = <VarInt as SliceSerializable<i32>>::write(bytes, *id);
                        bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, *y_offset);
                    },
                }
                bytes = <BigEndian as SliceSerializable<i32>>::write(bytes, *arrival_in_ticks);
            },
            Particle::Trail { target, color } => {
                bytes = <BigEndian as SliceSerializable<f64>>::write(bytes, target.0);
                bytes = <BigEndian as SliceSerializable<f64>>::write(bytes, target.1);
                bytes = <BigEndian as SliceSerializable<f64>>::write(bytes, target.2);
                bytes = <BigEndian as SliceSerializable<i32>>::write(bytes, *color);
            },
            Particle::ItemSlime => {},
            Particle::ItemCobweb => {},
            Particle::ItemSnowball => {},
            Particle::LargeSmoke => {},
            Particle::Lava => {},
            Particle::Mycelium => {},
            Particle::Note => {},
            Particle::Poof => {},
            Particle::Portal => {},
            Particle::Rain => {},
            Particle::Smoke => {},
            Particle::WhiteSmoke => {},
            Particle::Sneeze => {},
            Particle::Spit => {},
            Particle::SquidInk => {},
            Particle::SweepAttack => {},
            Particle::TotemOfUndying => {},
            Particle::Underwater => {},
            Particle::Splash => {},
            Particle::Witch => {},
            Particle::BubblePop => {},
            Particle::CurrentDown => {},
            Particle::BubbleColumnUp => {},
            Particle::Nautilus => {},
            Particle::Dolphin => {},
            Particle::CampfireCosySmoke => {},
            Particle::CampfireSignalSmoke => {},
            Particle::DrippingHoney => {},
            Particle::FallingHoney => {},
            Particle::LandingHoney => {},
            Particle::FallingNectar => {},
            Particle::FallingSporeBlossom => {},
            Particle::Ash => {},
            Particle::CrimsonSpore => {},
            Particle::WarpedSpore => {},
            Particle::SporeBlossomAir => {},
            Particle::DrippingObsidianTear => {},
            Particle::FallingObsidianTear => {},
            Particle::LandingObsidianTear => {},
            Particle::ReversePortal => {},
            Particle::WhiteAsh => {},
            Particle::SmallFlame => {},
            Particle::Snowflake => {},
            Particle::DrippingDripstoneLava => {},
            Particle::FallingDripstoneLava => {},
            Particle::DrippingDripstoneWater => {},
            Particle::FallingDripstoneWater => {},
            Particle::GlowSquidInk => {},
            Particle::Glow => {},
            Particle::WaxOn => {},
            Particle::WaxOff => {},
            Particle::ElectricSpark => {},
            Particle::Scrape => {},
            Particle::Shriek { delay } => {},
            Particle::EggCrack => {},
            Particle::DustPlume => {},
            Particle::TrialSpawnerDetection => {},
            Particle::TrialSpawnerDetectionOminous => {},
            Particle::VaultConnection => {},
            Particle::DustPillar { block_state } => {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, *block_state as i32);
            },
            Particle::OminousSpawning => {},
            Particle::RaidOmen => {},
            Particle::TrialOmen => {},
            Particle::BlockCrumble { block_state } => {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, *block_state as i32);
            },
        }
        bytes
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        let mut write_size = 0;
        let id = data.get_id();
        write_size += <VarInt as SliceSerializable<i32>>::get_write_size(id);

        match data {
            Particle::AngryVillager => {},
            Particle::Block { block_state } => {
                write_size += <VarInt as SliceSerializable<i32>>::get_write_size(*block_state as i32);
            },
            Particle::BlockMarker { block_state } => {
                write_size += <VarInt as SliceSerializable<i32>>::get_write_size(*block_state as i32);
            },
            Particle::Bubble => {},
            Particle::Cloud => {},
            Particle::Crit => {},
            Particle::DamageIndicator => {},
            Particle::DragonBreath => {},
            Particle::DrippingLava => {},
            Particle::FallingLava => {},
            Particle::LandingLava => {},
            Particle::DrippingWater => {},
            Particle::FallingWater => {},
            Particle::Dust { color, scale } => {
                write_size += <BigEndian as SliceSerializable<i32>>::get_write_size(*color);
                write_size += <BigEndian as SliceSerializable<f32>>::get_write_size(*scale);
            },
            Particle::DustColorTransition { from_color, to_color, scale } => {
                write_size += <BigEndian as SliceSerializable<i32>>::get_write_size(*from_color);
                write_size += <BigEndian as SliceSerializable<i32>>::get_write_size(*to_color);
                write_size += <BigEndian as SliceSerializable<f32>>::get_write_size(*scale);
            },
            Particle::Effect => {},
            Particle::ElderGuardian => {},
            Particle::EnchantedHit => {},
            Particle::Enchant => {},
            Particle::EndRod => {},
            Particle::EntityEffect { color } => {
                write_size += <BigEndian as SliceSerializable<i32>>::get_write_size(*color);
            },
            Particle::ExplosionEmitter => {},
            Particle::Explosion => {},
            Particle::Gust => {},
            Particle::SmallGust => {},
            Particle::GustEmitterLarge => {},
            Particle::GustEmitterSmall => {},
            Particle::SonicBoom => {},
            Particle::FallingDust { block_state } => {
                write_size += <VarInt as SliceSerializable<i32>>::get_write_size(*block_state as i32);
            },
            Particle::Firework => {},
            Particle::Fishing => {},
            Particle::Flame => {},
            Particle::Infested => {},
            Particle::CherryLeaves => {},
            Particle::SculkSoul => {},
            Particle::SculkCharge { roll } => {
                write_size += <BigEndian as SliceSerializable<f32>>::get_write_size(*roll);
            },
            Particle::SculkChargePop => {},
            Particle::SoulFireFlame => {},
            Particle::Soul => {},
            Particle::Flash => {},
            Particle::HappyVillager => {},
            Particle::Composter => {},
            Particle::Heart => {},
            Particle::InstantEffect => {},
            Particle::Item { encoded_item } => {
                write_size += WriteOnlyBlob::get_write_size(encoded_item);
            },
            Particle::Vibration { destination, arrival_in_ticks } => {
                match destination {
                    PositionSource::Block { x, y, z } => {
                        write_size += <Single as SliceSerializable<u8>>::get_write_size(builtin::PositionSourceType::Block as u8);
                        write_size += BlockPosition::get_write_size(BlockPosition::new(*x, *y, *z));

                    },
                    PositionSource::Entity { id, y_offset } => {
                        write_size += <Single as SliceSerializable<u8>>::get_write_size(builtin::PositionSourceType::Entity as u8);
                        write_size += <VarInt as SliceSerializable<i32>>::get_write_size(*id);
                        write_size += <BigEndian as SliceSerializable<f32>>::get_write_size(*y_offset);
                    },
                }
                write_size += <BigEndian as SliceSerializable<i32>>::get_write_size(*arrival_in_ticks);
            },
            Particle::Trail { target, color } => {
                write_size += <BigEndian as SliceSerializable<f64>>::get_write_size(target.0);
                write_size += <BigEndian as SliceSerializable<f64>>::get_write_size(target.1);
                write_size += <BigEndian as SliceSerializable<f64>>::get_write_size(target.2);
                write_size += <BigEndian as SliceSerializable<i32>>::get_write_size(*color);
            },
            Particle::ItemSlime => {},
            Particle::ItemCobweb => {},
            Particle::ItemSnowball => {},
            Particle::LargeSmoke => {},
            Particle::Lava => {},
            Particle::Mycelium => {},
            Particle::Note => {},
            Particle::Poof => {},
            Particle::Portal => {},
            Particle::Rain => {},
            Particle::Smoke => {},
            Particle::WhiteSmoke => {},
            Particle::Sneeze => {},
            Particle::Spit => {},
            Particle::SquidInk => {},
            Particle::SweepAttack => {},
            Particle::TotemOfUndying => {},
            Particle::Underwater => {},
            Particle::Splash => {},
            Particle::Witch => {},
            Particle::BubblePop => {},
            Particle::CurrentDown => {},
            Particle::BubbleColumnUp => {},
            Particle::Nautilus => {},
            Particle::Dolphin => {},
            Particle::CampfireCosySmoke => {},
            Particle::CampfireSignalSmoke => {},
            Particle::DrippingHoney => {},
            Particle::FallingHoney => {},
            Particle::LandingHoney => {},
            Particle::FallingNectar => {},
            Particle::FallingSporeBlossom => {},
            Particle::Ash => {},
            Particle::CrimsonSpore => {},
            Particle::WarpedSpore => {},
            Particle::SporeBlossomAir => {},
            Particle::DrippingObsidianTear => {},
            Particle::FallingObsidianTear => {},
            Particle::LandingObsidianTear => {},
            Particle::ReversePortal => {},
            Particle::WhiteAsh => {},
            Particle::SmallFlame => {},
            Particle::Snowflake => {},
            Particle::DrippingDripstoneLava => {},
            Particle::FallingDripstoneLava => {},
            Particle::DrippingDripstoneWater => {},
            Particle::FallingDripstoneWater => {},
            Particle::GlowSquidInk => {},
            Particle::Glow => {},
            Particle::WaxOn => {},
            Particle::WaxOff => {},
            Particle::ElectricSpark => {},
            Particle::Scrape => {},
            Particle::Shriek { delay } => {},
            Particle::EggCrack => {},
            Particle::DustPlume => {},
            Particle::TrialSpawnerDetection => {},
            Particle::TrialSpawnerDetectionOminous => {},
            Particle::VaultConnection => {},
            Particle::DustPillar { block_state } => {
                write_size += <VarInt as SliceSerializable<i32>>::get_write_size(*block_state as i32);
            },
            Particle::OminousSpawning => {},
            Particle::RaidOmen => {},
            Particle::TrialOmen => {},
            Particle::BlockCrumble { block_state } => {
                write_size += <VarInt as SliceSerializable<i32>>::get_write_size(*block_state as i32);
            },
        }
        write_size
    }
}