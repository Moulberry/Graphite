use glam::{DMat3, DMat4, DVec3, IVec3};
use graphite_binary::nbt::TAG_COMPOUND_ID;

use crate::{world::AnvilWorld, ChunkCoord, EntityNbtWithTransform};

pub fn load_anvil_entities(
    world: &AnvilWorld,
    folder: &include_dir::Dir,
) -> Result<Vec<EntityNbtWithTransform>, ()> {
    let mut output: Vec<EntityNbtWithTransform> = Vec::new();

    let min = ChunkCoord::new(world.min_chunk_x, world.min_chunk_z);
    let max = ChunkCoord::new(world.min_chunk_x + world.size_x as isize - 1,
        world.min_chunk_z + world.size_z as isize - 1);

    let offset_x = (world.min_chunk_x * -16) as f64;
    let offset_y = (world.min_chunk_y * -16) as f64;
    let offset_z = (world.min_chunk_z * -16) as f64;
    let offset = DVec3::new(offset_x, offset_y, offset_z);

    super::load_anvil(min, max, folder, "entities/", |_, _, chunk_data| {
        let Some(chunk_data) = chunk_data.as_compound() else {
            return;
        };
        if let Some(entities) = chunk_data.find_list("Entities", TAG_COMPOUND_ID) {
            for entity in entities.iter() {
                let Some(entity) = entity.as_compound() else {
                    continue;
                };

                output.push(EntityNbtWithTransform {
                    entity: entity.clone_nbt(),
                    transform: DMat4::from_translation(offset),
                });
            }
        }
    });

    Ok(output)
}