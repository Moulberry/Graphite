use common::{DummyUniverseService};
use protocol::play::server::{CustomPayload, SetChunkCacheCenter, SetPlayerPosition, AddEntity, RemoveEntities};
use server::{universe::{UniverseService, Universe, EntityId}, entity::{components::{BasicEntity, Viewable}, position::Coordinate}};
mod common;

// Connects a player to the world
// Checks to see that the player received the joining packets
#[test]
fn player_join() {
    let (_, mut conn) = common::mock_universe_and_player();

    // Login -- Can't read NBT yet
    conn.skip_outgoing();

    // Custom Payload
    conn.assert_outgoing(&CustomPayload {
        channel: "minecraft:brand",
        data: b"\x08Graphite",
    });

    // LevelChunkWithLight -- Can't read NBT yet
    for _ in 0..(17*17) {
        conn.skip_outgoing();
    }

    // SetChunkCacheCenter
    conn.assert_outgoing(&SetChunkCacheCenter {
        chunk_x: 2,
        chunk_z: 2
    });

    // SetPlayerPosition
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
    let (mut universe, mut conn) = common::mock_universe_and_player();

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
        assert_eq!(packet.x, entity_position.x as _);
        assert_eq!(packet.y, entity_position.y as _);
        assert_eq!(packet.z, entity_position.z as _);
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
    let (mut universe, mut conn) = common::mock_universe_and_player();

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
        assert_eq!(packet.x, entity_position.x as _);
        assert_eq!(packet.y, entity_position.y as _);
        assert_eq!(packet.z, entity_position.z as _);
    });
    conn.assert_none_outgoing(); // No more packets
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