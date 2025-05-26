use std::io::Write;

use convert_case::{Case, Casing};
use indexmap::IndexMap;

#[derive(serde_derive::Deserialize)]
struct Particle {
    particle_options: Option<String>
}

pub fn write_particles() -> anyhow::Result<()> {
    let raw_data = include_str!("../data/particles.json");
    let particles: IndexMap<String, Particle> = serde_json::from_str(raw_data)?;

    let mut write_buffer = String::new();

    // Particles
    write_buffer.push_str("use graphite_binary::slice_serialization::*;\n");
    write_buffer.push_str("slice_serializable! {\n");
    write_buffer.push_str("\t#[derive(Debug, Clone)]\n");
    write_buffer.push_str("\tpub enum Particle {\n");

    for (name, particle) in &particles {
        let pretty_name = name.to_case(Case::Pascal);

        if let Some(particle_options) = &particle.particle_options {
            write_buffer.push_str(&format!("\t\t{} {{\n", pretty_name));
            match particle_options.as_str() {
                "block" => {
                    write_buffer.push_str("\t\t\tblock_state: u16 as VarInt,\n");
                }
                "dust" => {
                    write_buffer.push_str("\t\t\tcolor: i32 as BigEndian,\n");
                    write_buffer.push_str("\t\t\tscale: f32 as BigEndian,\n");
                }
                "dust_color_transition" => {
                    write_buffer.push_str("\t\t\tfrom_color: i32 as BigEndian,\n");
                    write_buffer.push_str("\t\t\tto_color: i32 as BigEndian,\n");
                    write_buffer.push_str("\t\t\tscale: f32 as BigEndian,\n");
                }
                "color" => {
                    write_buffer.push_str("\t\t\tcolor: i32 as BigEndian,\n");
                }
                "sculk_charge" => {
                    write_buffer.push_str("\t\t\troll: f32 as BigEndian,\n");
                }
                "item" => {
                    write_buffer.push_str("\t\t\tencoded_item: Box<[u8]> as WriteOnlyBlob,\n");
                }
                "vibration" => {
                    write_buffer.push_str("\t\t\tdestination: PositionSource,\n");
                    write_buffer.push_str("\t\t\tarrival_in_ticks: i32 as BigEndian,\n");
                }
                "target_color" => {
                    write_buffer.push_str("\t\t\ttarget_x: f64 as BigEndian,\n");
                    write_buffer.push_str("\t\t\ttarget_y: f64 as BigEndian,\n");
                    write_buffer.push_str("\t\t\ttarget_z: f64 as BigEndian,\n");
                    write_buffer.push_str("\t\t\tcolor: i32 as BigEndian,\n");
                }
                "shriek" => {
                    write_buffer.push_str("\t\t\tdelay: i32 as VarInt,\n");
                }
                "trail" => {
                    write_buffer.push_str("\t\t\ttarget_x: f64 as BigEndian,\n");
                    write_buffer.push_str("\t\t\ttarget_y: f64 as BigEndian,\n");
                    write_buffer.push_str("\t\t\ttarget_z: f64 as BigEndian,\n");
                    write_buffer.push_str("\t\t\tcolor: i32 as BigEndian,\n");
                    write_buffer.push_str("\t\t\tduration: i32 as VarInt,\n");
                }
                _ => anyhow::bail!("Unknown particle option type: {}", particle_options)
            }
            write_buffer.push_str("\t\t},\n");
        } else {
            write_buffer.push_str(&format!("\t\t{},\n", pretty_name));
        }
    }

    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n\n");

    write_buffer.push_str(r#"
#[derive(Debug, Copy, Clone)]
pub enum PositionSource {
    Block { // Encoded as a long
        x: i32,
        y: i32,
        z: i32,
    },
    Entity {
        id: i32, // VarInt
        y_offset: f32, 
    }
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd> for PositionSource {
    type CopyType = PositionSource;

    #[inline(always)]
    fn as_copy_type(t: &'r PositionSource) -> Self::CopyType {
        *t
    }

    fn read(_: &mut &'d [u8]) -> anyhow::Result<PositionSource> {
        todo!("reading positionsource unimplemented")
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        match data {
            PositionSource::Block { x, y, z } => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, crate::builtin::PositionSourceType::Block as u8);
                
                let packed_position = ((x as i64 & 0x3FFFFFF) << 38)
                    | ((z as i64 & 0x3FFFFFF) << 12)
                    | (y as i64 & 0xFFF);
                bytes = <BigEndian as SliceSerializable<i64>>::write(bytes, packed_position);

            },
            PositionSource::Entity { id, y_offset } => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, crate::builtin::PositionSourceType::Entity as u8);
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, id);
                bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, y_offset);
            },
        }
        bytes
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        let mut write_size = 0;
        match data {
            PositionSource::Block { x, y, z } => {
                write_size += <Single as SliceSerializable<u8>>::get_write_size(crate::builtin::PositionSourceType::Block as u8);
                write_size += <BigEndian as SliceSerializable<i64>>::get_write_size(0)

            },
            PositionSource::Entity { id, y_offset } => {
                write_size += <Single as SliceSerializable<u8>>::get_write_size(crate::builtin::PositionSourceType::Entity as u8);
                write_size += <VarInt as SliceSerializable<i32>>::get_write_size(id);
                write_size += <BigEndian as SliceSerializable<f32>>::get_write_size(y_offset);
            },
        }
        write_size
    }
}
"#);

    let mut f = crate::file_src("particle.rs");
    f.write_all(write_buffer.as_bytes())?;

    Ok(())
}