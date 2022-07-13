use bytes::BufMut;
use protocol::play::server::{
    ChunkBlockData, ChunkLightData, LevelChunkWithLight, Login, SetChunkCacheCenter,
    SetPlayerPosition,
};

use crate::{
    player::{proto_player::ProtoPlayer, Player, PlayerService},
    position::Position,
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
    fn initialize(world: &World<Self>);
    fn tick(world: &mut World<Self>);
    fn get_player_count(world: &World<Self>) -> usize;
}

// graphite world

pub struct World<W: WorldService> {
    pub service: W,
    universe: *mut Universe<W::UniverseServiceType>,
}

#[derive(Debug, Clone, Copy)]
pub struct ChunkViewPosition {
    x: i32,
    z: i32,
}

// graphite world impl

impl<W: WorldService> World<W> {
    pub fn get_universe(&mut self) -> &mut Universe<W::UniverseServiceType> {
        unsafe { self.universe.as_mut().unwrap() }
    }

    pub fn new(service: W) -> Self {
        Self {
            service,
            universe: std::ptr::null_mut(),
        }
    }

    pub fn initialize(&self, universe: &Universe<W::UniverseServiceType>) {
        // todo: justify this
        unsafe {
            let self_mut: *mut World<W> = self as *const _ as *mut _;
            let self_mut_ref: &mut World<W> = self_mut.as_mut().unwrap();
            assert!(self_mut_ref.universe.is_null(), "cannot initialize twice");
            self_mut_ref.universe = universe as *const _ as *mut _;
        }

        W::initialize(self);
    }

    pub fn tick(&mut self) {
        W::tick(self);
    }

    pub fn send_player_to(&mut self, proto_player: ProtoPlayer<W::UniverseServiceType>) {
        // notify service
        W::handle_player_join(self, proto_player);
    }

    pub(crate) fn update_view_position<P: PlayerService>(
        &self,
        player: &mut Player<P>,
        position: Position,
    ) -> anyhow::Result<ChunkViewPosition> {
        // todo: send new chunks & entities

        let chunk_view_position = ChunkViewPosition {
            x: (position.coord.x / 16.0) as i32,
            z: (position.coord.z / 16.0) as i32,
        };

        if player.chunk_view_position.x == chunk_view_position.x && player.chunk_view_position.z == chunk_view_position.z {
            return Ok(player.chunk_view_position);
        }

        // Update view position
        let update_view_position_packet = SetChunkCacheCenter {
            chunk_x: chunk_view_position.x,
            chunk_z: chunk_view_position.z,
        };
        player.write_packet(&update_view_position_packet)?;

        Ok(chunk_view_position)
    }

    pub(crate) fn initialize_view_position(
        &mut self,
        proto_player: &mut ProtoPlayer<W::UniverseServiceType>,
        position: Position,
    ) -> anyhow::Result<ChunkViewPosition> {
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

                let chunk_packet = LevelChunkWithLight {
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
                net::packet_helper::write_packet(&mut proto_player.write_buffer, &chunk_packet)?;
            }
        }

        let chunk_view_position = ChunkViewPosition {
            x: (position.coord.x / 16.0) as i32,
            z: (position.coord.z / 16.0) as i32,
        };

        // Update view position
        let update_view_position_packet = SetChunkCacheCenter {
            chunk_x: chunk_view_position.x,
            chunk_z: chunk_view_position.z,
        };
        net::packet_helper::write_packet(
            &mut proto_player.write_buffer,
            &update_view_position_packet,
        )?;

        // Position
        let position_packet = SetPlayerPosition {
            x: position.coord.x as _,
            y: position.coord.y as _,
            z: position.coord.z as _,
            yaw: position.rot.yaw,
            pitch: position.rot.pitch,
            relative_arguments: 0,
            id: 0,
            dismount_vehicle: false,
        };
        net::packet_helper::write_packet(&mut proto_player.write_buffer, &position_packet)?;

        Ok(chunk_view_position)
    }

    pub(crate) fn write_game_join_packet(
        &mut self,
        //write_buffer: &mut WriteBuffer,
        proto_player: &mut ProtoPlayer<W::UniverseServiceType>,
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

        println!("codec bytes: {}", binary.len());

        let join_game_packet = Login {
            entity_id: proto_player.entity_id.as_i32(),
            is_hardcore: proto_player.hardcore,
            gamemode: 0,
            previous_gamemode: -1,
            dimension_names: vec!["minecraft:overworld"],
            registry_codec: &binary,
            dimension_type: "minecraft:overworld",
            dimension_name: "minecraft:overworld",
            hashed_seed: 0, // affects biome noise
            max_players: 0, // unused
            view_distance: 8,
            simulation_distance: 8,
            reduced_debug_info: false,
            enable_respawn_screen: false,
            is_debug: false,
            is_flat: false,
            has_death_location: false,
        };

        net::packet_helper::write_packet(&mut proto_player.write_buffer, &join_game_packet)
    }
}
