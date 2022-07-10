use bytes::BufMut;
use net::network_buffer::WriteBuffer;
use protocol::play::server::{
    ChunkBlockData, ChunkDataAndUpdateLight, ChunkLightData, JoinGame, PlayerPositionAndLook,
    UpdateViewPosition,
};

use crate::{
    proto_player::ProtoPlayer,
    universe::{Universe, UniverseService},
};

// user defined world service trait

pub trait WorldService
where
    Self: Sized,
{
    type UniverseServiceType: UniverseService;

    fn handle_player_join(
        world: &mut World<Self>,
        proto_player: ProtoPlayer<Self::UniverseServiceType>,
    );
    fn tick(world: &mut World<Self>);
}

// graphite world

pub struct World<W: WorldService> {
    pub service: W,
    universe: *mut Universe<W::UniverseServiceType>,
}

pub struct ChunkViewPosition(i32, i32);

// graphite world impl

impl<W: WorldService> World<W> {
    pub fn get_universe(&mut self) -> &mut Universe<W::UniverseServiceType> {
        unsafe { self.universe.as_mut().unwrap() }
    }

    pub(crate) fn new(service: W, universe: &mut Universe<W::UniverseServiceType>) -> Self {
        Self { service, universe }
    }

    pub fn tick(&mut self) {
        W::tick(self);
    }

    pub fn send_player_to(&mut self, proto_player: ProtoPlayer<W::UniverseServiceType>) {
        // send some default packets like join game etc.
        println!("got the connection, would send smth here");

        // notify service
        W::handle_player_join(self, proto_player);
    }

    pub(crate) fn initialize_view_position(
        &mut self,
        proto_player: &mut ProtoPlayer<W::UniverseServiceType>,
    ) -> anyhow::Result<ChunkViewPosition> {
        let spawn_point: (f64, f64, f64) = (0.0, 500.0, 0.0);

        let mut heightmap_nbt = quartz_nbt::NbtCompound::new();
        let mut motion_blocking_nbt = quartz_nbt::NbtList::new();
        for _ in 0..256 {
            motion_blocking_nbt.push(0_i64);
        }
        heightmap_nbt.insert("MOTION_BLOCKING", motion_blocking_nbt);

        let mut binary: Vec<u8> = Vec::new();
        quartz_nbt::io::write_nbt(
            &mut binary,
            None,
            &heightmap_nbt,
            quartz_nbt::io::Flavor::Uncompressed,
        )
        .unwrap();
        binary.shrink_to_fit();

        // Chunk
        for x in -5..5 {
            for z in -5..5 {
                let mut chunk_data = vec![0_u8; 0];
                for i in 0..24 {
                    chunk_data.put_i16(16 * 16 * 16); // block count

                    // blocks
                    chunk_data.put_u8(0); // single pallete, 0 bits per entry
                    if i < 18 && x + z != 0 {
                        chunk_data.put_u8(1); // palette. stone
                    } else {
                        chunk_data.put_u8(0); // palette. air
                    }
                    chunk_data.put_u8(0); // 0 size array

                    // biomes
                    chunk_data.put_u8(0); // single pallete, 0 bits per entry
                    chunk_data.put_u8(1); // some biome
                    chunk_data.put_u8(0); // 0 size array
                }

                let chunk_packet = ChunkDataAndUpdateLight {
                    chunk_x: x,
                    chunk_z: z,
                    chunk_block_data: ChunkBlockData {
                        heightmaps: &binary,
                        data: &chunk_data,
                        block_entity_count: 0,
                        trust_edges: false,
                    },
                    chunk_light_data: ChunkLightData {
                        sky_light_mask: vec![],
                        block_light_mask: vec![],
                        empty_sky_light_mask: vec![],
                        empty_block_light_mask: vec![],
                        sky_light_entries: vec![],
                        block_light_entries: vec![],
                    },
                };
                net::packet_helper::write_packet(&mut proto_player.write_buffer, &chunk_packet)
                    .unwrap();
            }
        }

        // Update view position
        let update_view_position_packet = UpdateViewPosition {
            chunk_x: 0,
            chunk_z: 0,
        };
        net::packet_helper::write_packet(
            &mut proto_player.write_buffer,
            &update_view_position_packet,
        )
        .unwrap();

        // Position
        let position_packet = PlayerPositionAndLook {
            x: 0.0,
            y: 500.0,
            z: 0.0,
            yaw: 15.0,
            pitch: 0.0,
            flags: 0,
            teleport_id: 0,
            dismount_vehicle: false,
        };
        net::packet_helper::write_packet(&mut proto_player.write_buffer, &position_packet)?;

        Ok(ChunkViewPosition {
            0: (spawn_point.0 / 16.0) as i32,
            1: (spawn_point.2 / 16.0) as i32,
        })
    }

    pub(crate) fn write_game_join_packet(
        &mut self,
        write_buffer: &mut WriteBuffer,
    ) -> anyhow::Result<()> {
        let registry_codec =
            quartz_nbt::snbt::parse(include_str!("../../assets/registry_codec.json")).unwrap();
        let mut binary: Vec<u8> = Vec::new();
        quartz_nbt::io::write_nbt(
            &mut binary,
            None,
            &registry_codec,
            quartz_nbt::io::Flavor::Uncompressed,
        )
        .unwrap();
        binary.shrink_to_fit();

        let join_game_packet = JoinGame {
            entity_id: 0,
            is_hardcore: false,
            gamemode: 1,
            previous_gamemode: -1,
            dimension_names: vec!["minecraft:overworld"],
            registry_codec: &binary,
            dimension_type: "minecraft:overworld",
            dimension_name: "minecraft:overworld",
            hashed_seed: 69,
            max_players: 100,
            view_distance: 8,
            simulation_distance: 8,
            reduced_debug_info: false,
            enable_respawn_screen: false,
            is_debug: false,
            is_flat: false,
            has_death_location: false,
        };

        net::packet_helper::write_packet(write_buffer, &join_game_packet)
    }
}
