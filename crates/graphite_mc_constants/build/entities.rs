use std::{cell::RefCell, fmt::Write as _};
use std::io::Write;

use convert_case::{Case, Casing};
use indexmap::IndexMap;
use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct EntityData {
    pub id: usize,
    pub translation_key: String,
    pub height: f32,
    pub width: f32,
    #[serde(default)]
    pub metadata: Vec<EntityMetadateEntry>
}


#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct EntityMetadateEntry {
    pub name: String,
    pub serializer: String
}

pub fn write_entities() -> anyhow::Result<()> {
    let raw_data = include_str!("../data/entities.json");
    let mut entities: IndexMap<String, EntityData> = serde_json::from_str(raw_data)?;

    entities.sort_by(|_, v1, _, v2| {
        v1.id.cmp(&v2.id)
    });

    let entity_count = entities.len();

    let mut write_buffer = String::new();

    // Item Enum
    write_buffer.push_str("#![allow(warnings, unused, unused_assignments)]\n\n");
    write_buffer.push_str("#[derive(Debug, Clone, Copy, Eq, PartialEq)]\n");
    write_buffer.push_str("#[repr(u8)]\n");
    write_buffer.push_str("pub enum Entity {\n");
    for (entity_name, entity_data) in &entities {
        writeln!(write_buffer, "\t{} = {},", entity_name.to_case(Case::Pascal), entity_data.id)?;
    }
    write_buffer.push_str("}\n");

    // Metadata
    write_buffer.push_str(r#"
#[derive(Debug, thiserror::Error)]
#[error("Invalid metadata changes")]
pub struct InvalidMetadataChanges;
pub trait Metadata {
    // fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
    fn read_changes(&mut self, bytes: &mut &[u8]) -> std::result::Result<(), InvalidMetadataChanges>;
    unsafe fn write_changes<'b>(&mut self, bytes: &'b mut [u8]) -> &'b mut [u8];
    fn get_write_size(&self) -> usize;

    fn write_metadata_changes_packet(&mut self, entity_id: i32, buffer: &mut graphite_network::PacketBuffer) -> std::result::Result<(), graphite_network::PacketWriteError> {
        let metadata_size = self.get_write_size();
        if metadata_size == 0 {
            return Ok(());
        }

        let expected_packet_size = 16 + metadata_size;
		use graphite_mc_protocol::IdentifiedPacket;
        buffer.write_custom(graphite_mc_protocol::play::clientbound::SetEntityData::ID as u8, expected_packet_size, |mut bytes| {
            unsafe {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, entity_id);
                bytes = self.write_changes(bytes);
            }
            bytes
        })
    }
}

#[derive(Default)]
enum MetadataChanges<const T: usize> {
    #[default]
    NoChanges,
    SingleChange {
        index: usize
    },
    ManyChanges {
        indices: [bool; T]
    }
}

impl<const T: usize> MetadataChanges<T> {
    fn mark_dirty(&mut self, index: usize) {
        match self {
            Self::NoChanges => {
                *self = Self::SingleChange { index }
            },
            Self::SingleChange { index: old_index } => {
                if *old_index != index {
                    let mut indices = [(); T].map(|_| Default::default());
                    indices[*old_index] = true;
                    indices[index] = true;
                    *self = Self::ManyChanges { indices }
                }
            },
            Self::ManyChanges { indices } => {
                indices[index] = true;
            }
        }
    }
}

use graphite_binary::slice_serialization::*;

"#);

    for (entity_name, entity_data) in &entities {
        let pascal_name = entity_name.to_case(Case::Pascal);

        let metadata = &entity_data.metadata;

        let mut lifetime = String::new();
        for entry in metadata {
            if entry.serializer == "item_stack" {
                lifetime.push_str("<'a>");
                break;
            }
        }

        writeln!(write_buffer, "#[readonly::make]")?;
        writeln!(write_buffer, "#[derive(Default)]")?;
        writeln!(
            write_buffer,
            "pub struct {}Metadata{} {{",
            pascal_name, lifetime
        )?;
        writeln!(
            write_buffer,
            "\tchanges: MetadataChanges<{}>,",
            metadata.len()
        )?;
        for entry in metadata {
            let mut name = entry.name.as_str();
            if name == "type" {
                name = "r#type";
            }

            writeln!(
                write_buffer,
                "\tpub {}: {},",
                name,
                serialize_type_to_rust_type(&entry.serializer)
            )?;
        }
        write_buffer.push_str("}\n\n");

        writeln!(
            write_buffer,
            "impl{} {}Metadata{} {{",
            lifetime, pascal_name, lifetime
        )?;
        for (index, entry) in metadata.iter().enumerate() {
            writeln!(
                write_buffer,
                "\tpub fn set_{}(&mut self, value: {}) {{",
                entry.name,
                serialize_type_to_rust_type(&entry.serializer)
            )?;

            let mut name = entry.name.as_str();
            if name == "type" {
                name = "r#type";
            }

            writeln!(write_buffer, "\t\tself.changes.mark_dirty({});", index)?;
            writeln!(write_buffer, "\t\tself.{} = value;", name)?;
            write_buffer.push_str("\t}\n");
        }

        write_buffer.push_str("\n\t#[inline(always)]\n");
        write_buffer
            .push_str("\tpub fn get_write_size_for_index(&self, index: usize) -> usize {\n");
        write_buffer.push_str("\t\tmatch index {\n");
        for (index, entry) in metadata.iter().enumerate() {
            let mut name = entry.name.as_str();
            if name == "type" {
                name = "r#type";
            }

            write!(write_buffer, "\t\t\t{} => ", index)?;
            write_buffer.push_str(&serialize_type_to_write_size(&entry.serializer, name));
            write_buffer.push_str(",\n");
        }
        write_buffer.push_str("\t\t\t_ => unreachable!()\n");
        write_buffer.push_str("\t\t}\n");
        write_buffer.push_str("\t}\n");

        write_buffer.push_str("\n\t#[inline(always)]\n");
        write_buffer.push_str("\tpub unsafe fn write_for_index<'b>(&self, mut bytes: &'b mut [u8], index: usize) -> &'b mut [u8] {\n");
        write_buffer.push_str("\t\tmatch index {\n");
        for (index, entry) in metadata.iter().enumerate() {
            let mut name = entry.name.as_str();
            if name == "type" {
                name = "r#type";
            }

            let serialize_id = serialize_type_to_id(&entry.serializer);

            writeln!(write_buffer, "\t\t\t{} => {{", index)?;
            writeln!(
                write_buffer,
                "\t\t\t\tbytes = <Single as SliceSerializable<u8>>::write(bytes, {});",
                index
            )?;
            writeln!(
                write_buffer,
                "\t\t\t\tbytes = <Single as SliceSerializable<u8>>::write(bytes, {});",
                serialize_id
            )?;
            write_buffer.push_str("\t\t\t\t");
            write_buffer.push_str(&serialize_type_to_write(&entry.serializer, name));
            write_buffer.push_str("\n\t\t\t},\n");
        }
        write_buffer.push_str("\t\t\t_ => unreachable!()\n");
        write_buffer.push_str("\t\t}\n");
        write_buffer.push_str("\t}\n");

        write_buffer.push_str("}\n\n");

        write!(
            write_buffer,
            "impl{} Metadata for {}Metadata{} {{",
            lifetime, pascal_name, lifetime
        )?;
        write_buffer.push_str(r#"
    /*fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }*/

    fn read_changes(&mut self, _bytes: &mut &[u8]) -> std::result::Result<(), InvalidMetadataChanges> {
        unimplemented!();
    }

    fn get_write_size(&self) -> usize {
        match self.changes {
            MetadataChanges::NoChanges => 0,
            MetadataChanges::SingleChange { index } => {
                1 + 2 + self.get_write_size_for_index(index)
            },
            MetadataChanges::ManyChanges { indices } => {
                let mut size = 1;
"#);
        for index in 0..metadata.len() {
            writeln!(
                write_buffer,
                "\t\t\t\tif indices[{}] {{ size += 2 + self.get_write_size_for_index({}); }}",
                index, index
            )?;
        }

        write_buffer.push_str(
            r#"                size
            }
        }
    }

    unsafe fn write_changes<'b>(&mut self, mut bytes: &'b mut [u8]) -> &'b mut [u8] {
        match self.changes {
            MetadataChanges::NoChanges => {},
            MetadataChanges::SingleChange { index } => {
                bytes = self.write_for_index(bytes, index);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 255);
            },
            MetadataChanges::ManyChanges { indices } => {
"#,
        );
        for index in 0..metadata.len() {
            writeln!(
                write_buffer,
                "\t\t\t\tif indices[{}] {{ bytes = self.write_for_index(bytes, {}); }}",
                index, index
            )?;
        }

        write_buffer.push_str(
            r#"                bytes = <Single as SliceSerializable<u8>>::write(bytes, 255);
            }
        }
        self.changes = MetadataChanges::NoChanges;
        bytes
    }
"#,
        );

//         if lifetime.is_empty() {
//             write!(
//                 write_buffer,
//                 "impl <'a> SliceSerializable<'a> for {}Metadata {{",
//                 pascal_name
//             )?;
//             write_buffer.push_str("\n\ttype CopyType = &'a Self;");
//         } else {
//             write!(
//                 write_buffer,
//                 "impl <'a> SliceSerializable<'a> for {}Metadata<'a> {{",
//                 pascal_name
//             )?;
//             write!(write_buffer, "\n\ttype CopyType = &'a {}Metadata<'a>;", pascal_name)?;
//         }
//         write_buffer.push_str(r#"

//     fn as_copy_type(t: &'a Self) -> Self::CopyType {
//         t
//     }

//     fn get_write_size(data: Self::CopyType) -> usize {
// 		let reference = data.changes.borrow();
//         match &*reference {
//             MetadataChanges::NoChanges => 0,
//             MetadataChanges::SingleChange { index } => {
//                 1 + 2 + data.get_write_size_for_index(*index)
//             },
//             MetadataChanges::ManyChanges { indices } => {
//                 let mut size = 1;
// "#);
//         for index in 0..metadata.len() {
//             writeln!(
//                 write_buffer,
//                 "\t\t\t\tif indices[{}] {{ size += 2 + data.get_write_size_for_index({}); }}",
//                 index, index
//             )?;
//         }

//         write_buffer.push_str(
//             r#"                size
//             }
//         }
//     }

//     fn read(_: &mut &'a [u8]) -> anyhow::Result<Self> {
//         unimplemented!()
//     }

//     unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
// 		let reference = data.changes.borrow();
//         match &*reference {
//             MetadataChanges::NoChanges => {},
//             MetadataChanges::SingleChange { index } => {
//                 bytes = data.write_for_index(bytes, *index);
//                 bytes = <Single as SliceSerializable<u8>>::write(bytes, 255);
//             },
//             MetadataChanges::ManyChanges { indices } => {
// "#,
//         );
//         for index in 0..metadata.len() {
//             writeln!(
//                 write_buffer,
//                 "\t\t\t\tif indices[{}] {{ bytes = data.write_for_index(bytes, {}); }}",
//                 index, index
//             )?;
//         }

//         write_buffer.push_str(
//             r#"                bytes = <Single as SliceSerializable<u8>>::write(bytes, 255);
//             }
//         }
//         drop(reference);

//         *data.changes.borrow_mut() = MetadataChanges::NoChanges;
//         bytes
//     }
// "#,
//         );

        write_buffer.push_str("}\n\n");

        // break;
    }

    write_buffer.push_str(
        r#"impl Entity {
    pub fn get_properties(self) -> &'static EntityProperties {
        &ENTITY_PROPERTIES_LUT[self as usize]
    }
}"#,
    );

    write_buffer.push_str("\n\n");

    // Item Properties Struct
    write_buffer.push_str("#[derive(Debug)]\n");
    write_buffer.push_str("pub struct EntityProperties {\n");
    write_buffer.push_str("\tpub width: f32,\n");
    write_buffer.push_str("\tpub height: f32,\n");
    write_buffer.push_str("}\n\n");

    writeln!(
        write_buffer,
        "const ENTITY_PROPERTIES_LUT: [EntityProperties; {}] = [",
        entity_count
    )?;
    for (entity_name, entity) in &entities {
        writeln!(write_buffer, "\tEntityProperties {{ // {}", entity_name)?;
        writeln!(write_buffer, "\t\twidth: {}_f32,", entity.width)?;
        writeln!(write_buffer, "\t\theight: {}_f32,", entity.height)?;
        write_buffer.push_str("\t},\n");
    }
    write_buffer.push_str("];\n\n");

    // NoSuchItemError
    write_buffer.push_str("#[derive(Debug, thiserror::Error)]\n");
    write_buffer.push_str("#[error(\"No entity exists for id: {0}\")]\n");
    write_buffer.push_str("pub struct NoSuchEntityError(u8);\n\n");

    // TryFrom<u16> for Item
    write_buffer.push_str("impl TryFrom<u8> for Entity {\n");
    write_buffer.push_str("\ttype Error = NoSuchEntityError;\n");
    write_buffer.push_str("\tfn try_from(value: u8) -> Result<Self, Self::Error> {\n");
    writeln!(
        write_buffer,
        "\t\tif value >= {} {{ return Err(NoSuchEntityError(value)); }}",
        entity_count
    )?;
    write_buffer.push_str("\t\tOk(unsafe { std::mem::transmute(value) })\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push('}');

    let mut f = crate::file_src("entity.rs");
    f.write_all(write_buffer.as_bytes())?;

    Ok(())
}

fn serialize_type_to_write(typ: &String, varname: &str) -> String {
    match typ.as_str() {
        "byte" => format!("<Single as SliceSerializable<u8>>::write(bytes, self.{varname})"),
        "int" => format!("<VarInt as SliceSerializable<i32>>::write(bytes, self.{varname})"),
        "long" => "unimplemented!()".into(),
        "float" => format!("<BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname})"),
        "string" => format!(
            "<SizedString<32767> as SliceSerializable<String>>::write(bytes, &self.{varname})"
        ),
        "component" => format!(
            "<NBTBlob as SliceSerializable<_>>::write(bytes, &self.{varname})"
        ),
        "optional_component" => format!(
            "<Option<NBTBlob> as SliceSerializable<_>>::write(bytes, &self.{varname}.as_ref().map(|v| std::borrow::Cow::Borrowed(v)))"
        ),
        "item_stack" => {
            format!("graphite_mc_protocol::types::ProtocolItemStack::write(bytes, &self.{varname})")
        }
        "boolean" => format!("<Single as SliceSerializable<bool>>::write(bytes, self.{varname})"),
        "rotations" => format!(
            "{{
            bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.0);
            bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.1);
            <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.2)
        }}"
        ),
        "block_pos" => "unimplemented!()".into(),
        "optional_block_pos" => "unimplemented!()".into(),
        "direction" => "unimplemented!()".into(),
        "optional_uuid" => "unimplemented!()".into(),
        "block_state" => format!("<VarInt as SliceSerializable<i32>>::write(bytes, self.{varname})"),
        "optional_block_state" => format!("<VarInt as SliceSerializable<i32>>::write(bytes, self.{varname}.unwrap_or(0))"),
        "compound_tag" => "unimplemented!()".into(),
        "particle" => "unimplemented!()".into(),
        "villager_data" => "unimplemented!()".into(),
        "optional_unsigned_int" => "unimplemented!()".into(),
        "pose" => format!("<Single as SliceSerializable<u8>>::write(bytes, self.{varname} as u8)"),
        "cat_variant" => "unimplemented!()".into(),
        "frog_variant" => "unimplemented!()".into(),
        "optional_global_pos" => "unimplemented!()".into(),
        "painting_variant" => "unimplemented!()".into(),
        "sniffer_state" => "unimplemented!()".into(),
        "vector3" => format!(
            "{{
            bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.0);
            bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.1);
            <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.2)
        }}"
        ),
        "quaternion" => format!(
            "{{
            bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.0);
            bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.1);
            bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.2);
            <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.3)
        }}"
        ),
        _ => panic!("unknown serialize type: {}", typ),
    }
}

fn serialize_type_to_id(typ: &String) -> usize {
    match typ.as_str() {
        "byte" => 0,
        "int" => 1,
        "long" => 2,
        "float" => 3,
        "string" => 4,
        "component" => 5,
        "optional_component" => 6,
        "item_stack" => 7,
        "boolean" => 8,
        "rotations" => 9,
        "block_pos" => 10,
        "optional_block_pos" => 11,
        "direction" => 12,
        "optional_uuid" => 13,
        "block_state" => 14,
        "optional_block_state" => 15,
        "compound_tag" => 16,
        "particle" => 17,
        "villager_data" => 18,
        "optional_unsigned_int" => 19,
        "pose" => 20,
        "cat_variant" => 21,
        "frog_variant" => 22,
        "optional_global_pos" => 23,
        "painting_variant" => 24,
        "sniffer_state" => 25,
        "vector3" => 26,
        "quaternion" => 27,
        _ => panic!("unknown serialize type: {}", typ),
    }
}

fn serialize_type_to_write_size(typ: &String, varname: &str) -> String {
    match typ.as_str() {
        "byte" => "1".into(),
        "int" => "5".into(),
        "long" => "unimplemented!()".into(),
        "float" => "4".into(),
        "string" => format!("5 + self.{varname}.len()"),
        "component" => format!("self.{varname}.to_bytes().len()"),
        "optional_component" => {
            format!("1 + if let Some(value) = &self.{varname} {{ value.to_bytes().len() }} else {{ 0 }}")
        }
        "item_stack" => {
            format!("graphite_mc_protocol::types::ProtocolItemStack::get_write_size(&self.{varname})")
        }
        "boolean" => "1".into(),
        "rotations" => "12".into(),
        "block_pos" => "8".into(),
        "optional_block_pos" => format!("1 + if self.{varname}.is_some() {{ 8 }} else {{ 0 }}"),
        "direction" => "1".into(),
        "optional_uuid" => format!("1 + if self.{varname}.is_some() {{ 16 }} else {{ 0 }}"),
        "block_state" => "5".into(),
        "optional_block_state" => "5".into(),
        "compound_tag" => "unimplemented!()".into(),
        "particle" => "unimplemented!()".into(),
        "villager_data" => "7".into(), // todo: add data type in protocol
        "optional_unsigned_int" => "5".into(),
        "pose" => "1".into(),
        "cat_variant" => "1".into(),
        "frog_variant" => "1".into(),
        "optional_global_pos" => format!(
            "1 + if let Some((world, _)) = &self.{varname} {{ 5 + world.len() + 8 }} else {{ 8 }}"
        ),
        "painting_variant" => "1".into(),
        "sniffer_state" => "unimplemented!()".into(),
        "vector3" => "12".into(),
        "quaternion" => "16".into(),
        _ => panic!("unknown serialize type: {}", typ),
    }
}

fn serialize_type_to_rust_type(typ: &String) -> &'static str {
    match typ.as_str() {
        "byte" => "u8",
        "int" => "i32",
        "long" => "()",
        "float" => "f32",
        "string" => "String",
        "component" => "graphite_binary::nbt::CachedNBT",
        "optional_component" => "Option<graphite_binary::nbt::CachedNBT>",
        "item_stack" => "graphite_mc_protocol::types::ProtocolItemStack<'a>",
        "boolean" => "bool",
        "rotations" => "(f32, f32, f32)",
        "block_pos" => "graphite_mc_protocol::types::BlockPosition",
        "optional_block_pos" => "Option<graphite_mc_protocol::types::BlockPosition>",
        "direction" => "graphite_mc_protocol::types::Direction",
        "optional_uuid" => "Option<u128>",
        "block_state" => "i32",
        "optional_block_state" => "Option<i32>",
        "compound_tag" => "()",
        "particle" => "()",
        "villager_data" => "(u8, u8, i32)", // todo: add data type in protocol
        "optional_unsigned_int" => "Option<u32>",
        "pose" => "graphite_mc_protocol::types::Pose",
        "cat_variant" => "u8",
        "frog_variant" => "u8",
        "optional_global_pos" => "Option<(String, graphite_mc_protocol::types::BlockPosition)>",
        "painting_variant" => "u8",
        "sniffer_state" => "()",
        "vector3" => "(f32, f32, f32)",
        "quaternion" => "(f32, f32, f32, f32)",
        _ => panic!("unknown serialize type: {}", typ),
    }
}
