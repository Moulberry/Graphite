use anyhow::bail;
use graphite_binary::nbt::NBTNode;
use graphite_command::dispatcher::RootDispatchNode;
use graphite_mc_constants::tags::block::BlockTags;
use graphite_net::network_handler::{
    ConnectionSlab, NetworkManagerService, NewConnectionAccepter, UninitializedConnection,
};
use graphite_mc_protocol::types::GameProfile;
use std::borrow::Cow;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::{sync::mpsc::Sender, time::Duration};

use graphite_mc_protocol::play::server::{Commands, CustomPayload, Tag, TagRegistry, UpdateTags, Login};

use crate::player::player_connection::{AbstractConnectionReference, PlayerConnection};
use crate::player::proto_player::ProtoPlayer;
use crate::ticker::UniverseTicker;

// user defined universe service trait

pub trait UniverseService: UniverseTicker<Self>
where
    Self: Sized + 'static,
{
    // todo: use default associated type of `ConnectionReference<Self>`
    type ConnectionReferenceType: AbstractConnectionReference<Self>;

    fn handle_player_join(universe: &mut Universe<Self>, proto_player: ProtoPlayer<Self>);
}

// graphite universe

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
#[repr(transparent)]
pub struct EntityId(i32);

impl EntityId {
    pub fn as_i32(&self) -> i32 {
        self.0
    }
}

pub struct Universe<U: UniverseService> {
    pub service: U,
    player_receiver: Receiver<(UninitializedConnection, GameProfile)>,
    entity_id_counter: i32,
    pub(crate) root_dispatch_node: Option<RootDispatchNode>,
    pub(crate) command_packet: Option<Commands>,
}

// graphite universe impl

impl<U: UniverseService> Universe<U> {
    pub fn handle_player_connect(
        &mut self,
        connection_ref: U::ConnectionReferenceType,
        profile: GameProfile,
    ) {
        let entity_id = self.new_entity_id();
        let mut proto_player = ProtoPlayer::new(connection_ref, profile, self.new_entity_id());

        self.write_login_packets(&mut proto_player);

        U::handle_player_join(self, proto_player);
    }

    fn write_login_packets(&mut self, proto_player: &mut ProtoPlayer<U>) {
        let mut nbt = graphite_binary::nbt::NBT::new();

        // Write minecraft:chat_type (empty)
        let chat_type_values = NBTNode::List { type_id: graphite_binary::nbt::TAG_COMPOUND_ID, children: Vec::new() };
        let mut chat_type = NBTNode::Compound(Default::default());
        nbt.insert(&mut chat_type, "type", NBTNode::String("minecraft:chat_type".into()));
        nbt.insert(&mut chat_type, "value", chat_type_values);
        nbt.insert_root("minecraft:chat_type", chat_type);

        // Write minecraft:dimension_type
        let mut my_dimension = NBTNode::Compound(Default::default());
        nbt.insert(&mut my_dimension, "ambient_light", NBTNode::Float(1.0));
        nbt.insert(&mut my_dimension, "fixed_time", NBTNode::Long(6000));
        nbt.insert(&mut my_dimension, "natural", NBTNode::Byte(1));
        nbt.insert(&mut my_dimension, "min_y", NBTNode::Int(0));
        nbt.insert(&mut my_dimension, "height", NBTNode::Int(384));
        // nbt.insert(&mut my_dimension, "effects", NBTNode::Byte(0));

        // These values don't affect the client, only the server. The values are meaningless
        nbt.insert(&mut my_dimension, "piglin_safe", NBTNode::Byte(0));
        nbt.insert(&mut my_dimension, "has_raids", NBTNode::Byte(0));
        nbt.insert(&mut my_dimension, "monster_spawn_light_level", NBTNode::Int(0));
        nbt.insert(&mut my_dimension, "monster_spawn_block_light_limit", NBTNode::Int(0));
        nbt.insert(&mut my_dimension, "infiniburn", NBTNode::String("#minecraft:infiniburn_overworld".into()));
        nbt.insert(&mut my_dimension, "respawn_anchor_works", NBTNode::Byte(0));
        nbt.insert(&mut my_dimension, "has_skylight", NBTNode::Byte(0));
        nbt.insert(&mut my_dimension, "bed_works", NBTNode::Byte(0));
        nbt.insert(&mut my_dimension, "logical_height", NBTNode::Int(384));
        nbt.insert(&mut my_dimension, "coordinate_scale", NBTNode::Double(1.0));
        nbt.insert(&mut my_dimension, "ultrawarm", NBTNode::Byte(0));
        nbt.insert(&mut my_dimension, "has_ceiling", NBTNode::Byte(0));

        let mut my_dimension_entry = NBTNode::Compound(Default::default());
        nbt.insert(&mut my_dimension_entry, "name", NBTNode::String("graphite:default_dimension".into()));
        nbt.insert(&mut my_dimension_entry, "id", NBTNode::Int(0));
        nbt.insert(&mut my_dimension_entry, "element", my_dimension.clone());

        // todo: remove this
        let mut my_dimension_entry_2 = NBTNode::Compound(Default::default());
        nbt.insert(&mut my_dimension_entry_2, "name", NBTNode::String("graphite:default_dimension2".into()));
        nbt.insert(&mut my_dimension_entry_2, "id", NBTNode::Int(0));
        nbt.insert(&mut my_dimension_entry_2, "element", my_dimension);

        let mut dimension_type_values = NBTNode::List { type_id: graphite_binary::nbt::TAG_COMPOUND_ID, children: Vec::new() };
        nbt.append(&mut dimension_type_values, my_dimension_entry);
        nbt.append(&mut dimension_type_values, my_dimension_entry_2);

        let mut dimension_type = NBTNode::Compound(Default::default());
        nbt.insert(&mut dimension_type, "type", NBTNode::String("minecraft:dimension_type".into()));
        nbt.insert(&mut dimension_type, "value", dimension_type_values);
        nbt.insert_root("minecraft:dimension_type", dimension_type);

        // Write minecraft:worldgen/biome
        let mut my_biome_effects = NBTNode::Compound(Default::default());
        nbt.insert(&mut my_biome_effects, "sky_color", NBTNode::Int(0x78a7ff));
        nbt.insert(&mut my_biome_effects, "water_fog_color", NBTNode::Int(0x050533));
        nbt.insert(&mut my_biome_effects, "water_color", NBTNode::Int(0x3f76e4));
        nbt.insert(&mut my_biome_effects, "fog_color", NBTNode::Int(0xc0d8ff));

        let mut my_biome = NBTNode::Compound(Default::default());
        nbt.insert(&mut my_biome, "precipitation", NBTNode::String("rain".into()));
        nbt.insert(&mut my_biome, "temperature", NBTNode::Float(0.8));
        nbt.insert(&mut my_biome, "downfall", NBTNode::Float(0.4));
        nbt.insert(&mut my_biome, "effects", my_biome_effects);

        let mut my_biome_entry = NBTNode::Compound(Default::default());
        nbt.insert(&mut my_biome_entry, "name", NBTNode::String("minecraft:plains".into()));
        nbt.insert(&mut my_biome_entry, "id", NBTNode::Int(0));
        nbt.insert(&mut my_biome_entry, "element", my_biome);

        let mut biome_type_values = NBTNode::List { type_id: graphite_binary::nbt::TAG_COMPOUND_ID, children: Vec::new() };
        nbt.append(&mut biome_type_values, my_biome_entry);

        let mut biome_type = NBTNode::Compound(Default::default());
        nbt.insert(&mut biome_type, "type", NBTNode::String("minecraft:worldgen/biome".into()));
        nbt.insert(&mut biome_type, "value", biome_type_values);
        nbt.insert_root("minecraft:worldgen/biome", biome_type);

        let join_game_packet = Login {
            entity_id: proto_player.entity_id.as_i32(),
            is_hardcore: proto_player.hardcore,
            gamemode: proto_player.abilities.gamemode as u8,
            previous_gamemode: -1,
            dimension_names: vec!["graphite:default_dimension"],
            registry_codec: Cow::Owned(nbt.into()),
            dimension_type: "graphite:default_dimension",
            dimension_name: "graphite:default_dimension",
            hashed_seed: 0, // affects biome noise
            max_players: 0, // unused
            view_distance: 8, //W::CHUNK_VIEW_DISTANCE as _,
            simulation_distance: 8, //W::ENTITY_VIEW_DISTANCE as _,
            reduced_debug_info: false,
            enable_respawn_screen: false,
            is_debug: false,
            is_flat: false,
            death_location: None,
        };

        graphite_net::packet_helper::try_write_packet(&mut proto_player.write_buffer, &join_game_packet);

        if let Some(command_packet) = &self.command_packet {
            graphite_net::packet_helper::try_write_packet(&mut proto_player.write_buffer, command_packet);
        }

        let mut block_registry: Vec<Tag> = Vec::new();
        for block_tag in BlockTags::iter() {
            let tag_name = block_tag.to_namespace();
            let tag_values = block_tag.values();
            block_registry.push(Tag {
                name: tag_name,
                entries: tag_values.into(),
            })
        }

        let mut registries: Vec<TagRegistry> = Vec::new();
        registries.push(TagRegistry {
            tag_type: "block",
            values: block_registry,
        });
        graphite_net::packet_helper::try_write_packet(&mut proto_player.write_buffer, &UpdateTags {
            registries,
        });

        let brand_packet = CustomPayload {
            channel: "minecraft:brand",
            data: b"\x08Graphite",
        };
        graphite_net::packet_helper::try_write_packet(&mut proto_player.write_buffer, &brand_packet);
    }

    pub fn new_entity_id(&mut self) -> EntityId {
        self.entity_id_counter = self.entity_id_counter.wrapping_add(1);
        EntityId(self.entity_id_counter)
    }
}

// network service impl

impl<U: UniverseService> NetworkManagerService for Universe<U> {
    const TICK_RATE: Option<std::time::Duration> = Some(Duration::from_millis(50));
    type ConnectionServiceType = PlayerConnection<U>;

    fn new_connection_service(&mut self) -> PlayerConnection<U> {
        unimplemented!();
    }

    fn tick(
        &mut self,
        connections: &mut ConnectionSlab<Self>,
        accepter: NewConnectionAccepter<Self>,
    ) -> anyhow::Result<()> {
        // Accept pending connections
        loop {
            match self.player_receiver.try_recv() {
                Ok(received) => {
                    let connection = received.0;

                    let connection_index = accepter.accept_and_get_index(
                        connection,
                        PlayerConnection::new(),
                        connections,
                    )?;
                    let connection_ref = U::ConnectionReferenceType::new_from_connection(
                        connections,
                        connection_index,
                    );
                    self.handle_player_connect(connection_ref, received.1);
                }
                Err(err) if err == TryRecvError::Disconnected => {
                    bail!("receiver was disconnected!");
                }
                Err(_) => {
                    break;
                }
            }
        }
        self.service.tick();
        Ok(())
    }
}

impl<U: UniverseService> Universe<U> {
    pub fn create_dummy(service: U) -> Universe<U> {
        let (_, rx) = mpsc::channel::<(UninitializedConnection, GameProfile)>();

        Universe {
            service,
            player_receiver: rx,
            entity_id_counter: 0,
            root_dispatch_node: None,
            command_packet: None,
        }
    }

    pub fn create_and_start<F: FnOnce() -> U + std::marker::Send + 'static>(
        service_func: F,
        commands: Option<(RootDispatchNode, Commands)>,
    ) -> Sender<(UninitializedConnection, GameProfile)> {
        let (tx, rx) = mpsc::channel::<(UninitializedConnection, GameProfile)>();

        std::thread::spawn(|| {
            let (root_dispatch_node, command_packet) = if let Some(commands) = commands {
                (Some(commands.0), Some(commands.1))
            } else {
                (None, None)
            };

            let service = service_func();
            let universe = Universe {
                service,
                player_receiver: rx,
                entity_id_counter: 0,
                root_dispatch_node,
                command_packet,
            };

            graphite_net::network_handler::start_with_init(universe, None, |network_manager| {
                let universe = &mut network_manager.service as *mut _;
                network_manager.service.service.update_children_ptr(universe);
            })
            .unwrap();
        });

        tx
    }
}
