use std::fmt::Write as _;
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
    pub packet_type: String,
    pub height: f32,
    pub width: f32,
    pub client_tracking_range: usize,
    #[serde(default)]
    pub metadata: IndexMap<String, String>,
}

pub fn write_entities() -> anyhow::Result<()> {
    let raw_data = include_str!("../data/entities.json");
    let entities: IndexMap<String, EntityData> = serde_json::from_str(raw_data)?;
    let entity_count = entities.len();

    let mut write_buffer = String::new();

    // Item Enum
    write_buffer.push_str("#![allow(warnings, unused, unused_assignments)]\n\n");
    write_buffer.push_str("#[derive(Debug, Clone, Copy, Eq, PartialEq)]\n");
    write_buffer.push_str("#[repr(u8)]\n");
    write_buffer.push_str("pub enum Entity {\n");
    for (entity_name, _) in &entities {
        writeln!(write_buffer, "\t{},", entity_name.to_case(Case::Pascal))?;
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

use binary::slice_serialization::*;

"#);

    for (entity_name, entity_data) in &entities {
        let pascal_name = entity_name.to_case(Case::Pascal);

        let metadata = &entity_data.metadata;

        let mut lifetime = String::new();
        for (_, metatype) in metadata {
            if metatype == "item_stack" {
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
        for (name, typ) in metadata {
            let mut name = name.as_str();
            if name == "type" {
                name = "r#type";
            }

            writeln!(
                write_buffer,
                "\tpub {}: {},",
                name,
                serialize_type_to_rust_type(typ)
            )?;
        }
        write_buffer.push_str("}\n\n");

        writeln!(
            write_buffer,
            "impl{} {}Metadata{} {{",
            lifetime, pascal_name, lifetime
        )?;
        for (index, (name, typ)) in metadata.iter().enumerate() {
            writeln!(
                write_buffer,
                "\tpub fn set_{}(&mut self, value: {}) {{",
                name,
                serialize_type_to_rust_type(typ)
            )?;

            let mut name = name.as_str();
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
        for (index, (name, typ)) in metadata.iter().enumerate() {
            let mut name = name.as_str();
            if name == "type" {
                name = "r#type";
            }

            write!(write_buffer, "\t\t\t{} => ", index)?;
            write_buffer.push_str(&serialize_type_to_write_size(typ, name));
            write_buffer.push_str(",\n");
        }
        write_buffer.push_str("\t\t\t_ => unreachable!()\n");
        write_buffer.push_str("\t\t}\n");
        write_buffer.push_str("\t}\n");

        write_buffer.push_str("\n\t#[inline(always)]\n");
        write_buffer.push_str("\tpub unsafe fn write_for_index<'b>(&self, mut bytes: &'b mut [u8], index: usize) -> &'b mut [u8] {\n");
        write_buffer.push_str("\t\tmatch index {\n");
        for (index, (name, typ)) in metadata.iter().enumerate() {
            let mut name = name.as_str();
            if name == "type" {
                name = "r#type";
            }

            let serialize_id = serialize_type_to_id(typ);

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
            write_buffer.push_str(&serialize_type_to_write(typ, name));
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
        "int" => format!("VarInt::write(bytes, self.{varname})"),
        "float" => format!("<BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname})"),
        "string" => format!(
            "<SizedString<32767> as SliceSerializable<String>>::write(bytes, &self.{varname})"
        ),
        "component" => format!(
            "<SizedString<32767> as SliceSerializable<String>>::write(bytes, &self.{varname})"
        ),
        "optional_component" => format!(
            "<Option<SizedString<32767>> as SliceSerializable<_>>::write(bytes, &self.{varname})"
        ),
        "item_stack" => {
            format!("protocol::types::ProtocolItemStack::write(bytes, &self.{varname})")
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
        "block_state" => "unimplemented!()".into(),
        "compound_tag" => "unimplemented!()".into(),
        "particle" => "unimplemented!()".into(),
        "villager_data" => "unimplemented!()".into(),
        "optional_unsigned_int" => "unimplemented!()".into(),
        "pose" => format!("<Single as SliceSerializable<u8>>::write(bytes, self.{varname} as u8)"),
        "cat_variant" => "unimplemented!()".into(),
        "frog_variant" => "unimplemented!()".into(),
        "optional_global_pos" => "unimplemented!()".into(),
        "painting_variant" => "unimplemented!()".into(),
        _ => panic!("unknown serialize type: {}", typ),
    }
}

fn serialize_type_to_id(typ: &String) -> usize {
    match typ.as_str() {
        "byte" => 0,
        "int" => 1,
        "float" => 2,
        "string" => 3,
        "component" => 4,
        "optional_component" => 5,
        "item_stack" => 6,
        "boolean" => 7,
        "rotations" => 8,
        "block_pos" => 9,
        "optional_block_pos" => 10,
        "direction" => 11,
        "optional_uuid" => 12,
        "block_state" => 13,
        "compound_tag" => 14,
        "particle" => 15,
        "villager_data" => 16,
        "optional_unsigned_int" => 17,
        "pose" => 18,
        "cat_variant" => 19,
        "frog_variant" => 20,
        "optional_global_pos" => 21,
        "painting_variant" => 22,
        _ => panic!("unknown serialize type: {}", typ),
    }
}

fn serialize_type_to_write_size(typ: &String, varname: &str) -> String {
    match typ.as_str() {
        "byte" => "1".into(),
        "int" => "5".into(),
        "float" => "4".into(),
        "string" => format!("5 + self.{varname}.len()"),
        "component" => format!("5 + self.{varname}.len()"),
        "optional_component" => {
            format!("1 + if let Some(value) = &self.{varname} {{ 5 + value.len() }} else {{ 0 }}")
        }
        "item_stack" => {
            format!("protocol::types::ProtocolItemStack::get_write_size(&self.{varname})")
        }
        "boolean" => "1".into(),
        "rotations" => "12".into(),
        "block_pos" => "8".into(),
        "optional_block_pos" => format!("1 + if self.{varname}.is_some() {{ 8 }} else {{ 0 }}"),
        "direction" => "1".into(),
        "optional_uuid" => format!("1 + if self.{varname}.is_some() {{ 16 }} else {{ 0 }}"),
        "block_state" => "5".into(),
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
        _ => panic!("unknown serialize type: {}", typ),
    }
}

fn serialize_type_to_rust_type(typ: &String) -> &'static str {
    match typ.as_str() {
        "byte" => "u8",
        "int" => "i32",
        "float" => "f32",
        "string" => "String",
        "component" => "String",
        "optional_component" => "Option<String>",
        "item_stack" => "protocol::types::ProtocolItemStack<'a>",
        "boolean" => "bool",
        "rotations" => "(f32, f32, f32)",
        "block_pos" => "protocol::types::BlockPosition",
        "optional_block_pos" => "Option<protocol::types::BlockPosition>",
        "direction" => "protocol::types::Direction",
        "optional_uuid" => "Option<u128>",
        "block_state" => "Option<i32>",
        "compound_tag" => "()",
        "particle" => "()",
        "villager_data" => "(u8, u8, i32)", // todo: add data type in protocol
        "optional_unsigned_int" => "Option<u32>",
        "pose" => "protocol::types::Pose",
        "cat_variant" => "u8",
        "frog_variant" => "u8",
        "optional_global_pos" => "Option<(String, protocol::types::BlockPosition)>",
        "painting_variant" => "u8",
        _ => panic!("unknown serialize type: {}", typ),
    }
}
