use graphite_binary::nbt::{NBT, TAG_COMPOUND_ID, TAG_DOUBLE_ID};

use crate::{world::AnvilWorld, ChunkCoord};

pub fn load_anvil_entities(
    world: &AnvilWorld,
    folder: include_dir::Dir,
) -> Result<Vec<NBT>, ()> {
    let mut output: Vec<NBT> = Vec::new();

    let min = ChunkCoord::new(world.min_chunk_x, world.min_chunk_z);
    let max = ChunkCoord::new(world.min_chunk_x + world.size_x as isize - 1,
        world.min_chunk_z + world.size_z as isize - 1);

    let offset_x = (world.min_chunk_x * -16) as f64;
    let offset_y = (world.min_chunk_y * -16) as f64;
    let offset_z = (world.min_chunk_z * -16) as f64;

    super::load_anvil(min, max, folder, |_, _, chunk_data| {
        if let Some(entities) = chunk_data.find_list("Entities", TAG_COMPOUND_ID) {
            for entity in entities.iter() {
                let entity = entity.as_compound().unwrap();

                let mut nbt = entity.clone_nbt();

                if let Some(mut value) = nbt.find_list_mut("Pos", TAG_DOUBLE_ID) {
                    let x = *value.get_double(0).unwrap();
                    let y = *value.get_double(1).unwrap();
                    let z = *value.get_double(2).unwrap();
                    value.insert_double_at(0, x + offset_x);
                    value.insert_double_at(1, y + offset_y);
                    value.insert_double_at(2, z + offset_z);
                }

                output.push(nbt);
            }
        }
    });

    Ok(output)
}