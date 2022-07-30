use common::{DummyUniverseService};
use protocol::play::server::*;
use server::{universe::{UniverseService, Universe, EntityId}, entity::{components::{BasicEntity, Viewable}, position::Coordinate}};

mod common;

// todo: add tests for entity/player moving simultaneously in/out of render distance

// Connects a player to the world
// Checks to see that the player received the joining packets
#[test]
fn player_join() {
    let (_, mut conn) = common::create_universe_and_player();

    // Login -- Can't read NBT yet
    log!("Skipping login packet...");
    conn.skip_outgoing(PacketId::Login as u8);

    // Custom Payload
    log!("Checking custom payload packet...");
    conn.assert_outgoing(&CustomPayload {
        channel: "minecraft:brand",
        data: b"\x08Graphite",
    });

    // PlayerInfo
    log!("Checking PlayerInfo::AddPlayer packet...");
    conn.assert_outgoing(&PlayerInfo::AddPlayer {
        values: vec![PlayerInfoAddPlayer {
            profile: common::create_game_profile(),
            gamemode: 1,
            ping: 0,
            display_name: None,
            signature_data: None
        }],
    });

    // LevelChunkWithLight -- Can't read NBT yet
    log!("Checking chunk packets...");
    for _ in 0..(17*17) {
        conn.skip_outgoing(PacketId::LevelChunkWithLight as u8);
    }

    // SetChunkCacheCenter
    log!("Checking SetChunkCacheCenter packet...");
    conn.assert_outgoing(&SetChunkCacheCenter {
        chunk_x: 2,
        chunk_z: 2
    });

    // SetPlayerPosition
    log!("Checking SetPlayerPosition packet...");
    conn.assert_outgoing(&SetPlayerPosition {
        x: 40.0,
        y: 224.0,
        z: 40.0,
        yaw: 0.0,
        pitch: 0.0,
        relative_arguments: 0,
        id: 0,
        dismount_vehicle: false
    });

    // No more packets
    log!("Checking for no more packets...");
    conn.assert_none_outgoing();
}

// Does the following:
//  a. Spawn an entity within viewing distance of the Player
//  b. Move the entity outside of the viewing distance
// Checks to see that:
//  1. Player receives AddEntity on the 1st tick
//  2. Player receives RemoveEntity on the 2nd tick
#[test]
fn spawn_entity_close() {
    let (mut universe, mut conn) = common::create_universe_and_player();

    // Skip joining packets
    conn.skip_all_outgoing();

    // (a) Spawn an entity within viewing distance of the Player
    let entity_position = Coordinate {
        x: 49.0,
        y: 224.0,
        z: 31.0,
    };
    let entity_id = spawn_entity_at(&mut universe, entity_position);

    // (1) Player receives AddEntity on the 1st tick
    DummyUniverseService::tick(&mut universe);
    conn.assert_outgoing_as::<AddEntity, _>(|packet| {
        assert_eq!(packet.id, entity_id.as_i32());
        assert_eq!(packet.entity_type, 6);
        assert_eq!(packet.x, entity_position.x as f64);
        assert_eq!(packet.y, entity_position.y as f64);
        assert_eq!(packet.z, entity_position.z as f64);
    });
    conn.assert_none_outgoing(); // No more packets

    // (b) Move the entity outside of the viewing distance
    let entity_position = Coordinate {
        x: 65.0, // out of view distance
        y: 224.0,
        z: 31.0,
    };
    move_entity_to(&mut universe, entity_id, entity_position);

    // (2) Player receives RemoveEntity on the 2nd tick
    DummyUniverseService::tick(&mut universe);
    conn.assert_outgoing(&RemoveEntities {
        entities: vec![entity_id.as_i32()]
    });
    conn.assert_none_outgoing(); // No more packets
}

// Performs the following:
//  a. Spawn an entity outside of the view distance of the Player
//  b. Move the entity within viewing distance
// Checks to see that:
//  1. Player *DOES NOT* receive AddEntity on the 1st tick
//  2. Player *DOES* receive AddEntity on the 2nd tick
#[test]
fn spawn_entity_far() {
    let (mut universe, mut conn) = common::create_universe_and_player();

    // Skip joining packets
    conn.skip_all_outgoing();

    // (a) Spawn an entity outside of the view distance of the Player
    let entity_position = Coordinate {
        x: 65.0, // out of view distance
        y: 224.0,
        z: 31.0,
    };
    let entity_id = spawn_entity_at(&mut universe, entity_position);

    // (1) Player *DOES NOT* receive AddEntity on the 1st tick
    DummyUniverseService::tick(&mut universe);
    conn.assert_none_outgoing();

    // (b) Move the entity within viewing distance
    let entity_position = Coordinate {
        x: 40.0, // within viewing distance
        y: 224.0,
        z: 31.0,
    };
    move_entity_to(&mut universe, entity_id, entity_position);

    // (2) Player *DOES* receive AddEntity on the 2nd tick
    DummyUniverseService::tick(&mut universe);
    conn.assert_outgoing_as::<AddEntity, _>(|packet| {
        assert_eq!(packet.id, entity_id.as_i32());
        assert_eq!(packet.entity_type, 6);
        assert_eq!(packet.x, entity_position.x as f64);
        assert_eq!(packet.y, entity_position.y as f64);
        assert_eq!(packet.z, entity_position.z as f64);
    });
    conn.assert_none_outgoing(); // No more packets
}

// Does the following:
//  a. Spawn an entity
//  b. Connect a player at a nearby location
//  c. Disconnect the player
//  d. Move the entity outside of the viewing distance
//  e. Connect a player at far away location
// Checks to see that:
//  1. Player receives AddEntity on the 1st login
//  2. Player does *NOT* receive AddEntity on the 2nd login
#[test]
fn spawn_entity_before() {
    let mut universe = common::create_universe();

    // (a) Spawn an entity within viewing distance
    let entity_position = Coordinate {
        x: 49.0,
        y: 224.0,
        z: 31.0,
    };
    let entity_id = spawn_entity_at(&mut universe, entity_position);

    // (b) Connect the player at a nearby location
    let mut conn = common::create_player(&mut universe);
    
    // Skip login packets
    conn.skip_outgoing(PacketId::Login as u8);
    conn.skip_outgoing(PacketId::CustomPayload as u8);
    conn.skip_outgoing(PacketId::PlayerInfo as u8);
    for _ in 0..(17*17) {
        conn.skip_outgoing(PacketId::LevelChunkWithLight as u8);
    }

    conn.assert_outgoing_as::<AddEntity, _>(|packet| {
        assert_eq!(packet.id, entity_id.as_i32());
        assert_eq!(packet.entity_type, 6);
        assert_eq!(packet.x, entity_position.x as f64);
        assert_eq!(packet.y, entity_position.y as f64);
        assert_eq!(packet.z, entity_position.z as f64);
    });

    // (c) Disconnect the player
    conn.disconnect();

    // (d) Move the entity outside of the viewing distance
    let entity_position = Coordinate {
        x: 65.0, // out of view distance
        y: 224.0,
        z: 31.0,
    };
    move_entity_to(&mut universe, entity_id, entity_position);
    DummyUniverseService::tick(&mut universe); // Tick to update entity position

    // (e) Connect a player at far away location
    let mut conn = common::create_player(&mut universe);

    // Skip initial login packets
    conn.skip_outgoing(PacketId::Login as u8);
    conn.skip_outgoing(PacketId::CustomPayload as u8);
    conn.skip_outgoing(PacketId::PlayerInfo as u8);
    for _ in 0..(17*17) {
        conn.skip_outgoing(PacketId::LevelChunkWithLight as u8);
    }

    // (2) Player does *NOT* receive AddEntity on the 2nd login

    // Skip remaining login packets
    conn.skip_outgoing(PacketId::SetChunkCacheCenter as u8);
    conn.skip_outgoing(PacketId::SetPlayerPosition as u8);
    conn.assert_none_outgoing();


}

// Helper functions

fn spawn_entity_at(universe: &mut Universe<DummyUniverseService>, position: Coordinate) -> EntityId {
    let entity_id = universe.new_entity_id();
    let test_entity = BasicEntity {
        entity_id,
        entity_type: 6,
    };
    universe.service.the_world.push_entity((), position, test_entity, entity_id);
    entity_id
}

fn move_entity_to(universe: &mut Universe<DummyUniverseService>, entity_id: EntityId, coordinate: Coordinate) {
    let mut entity = universe.service.the_world.get_entity_mut(entity_id)
        .expect("entity must exist");
    let mut viewable = entity.get_mut::<Viewable>()
        .expect("entity must have viewable component");
    viewable.coord.x = coordinate.x;
    viewable.coord.y = coordinate.y;
    viewable.coord.z = coordinate.z;
}