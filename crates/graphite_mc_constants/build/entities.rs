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
    pub interpolation_duration: usize,
    pub is_living_entity: bool,
    pub dimensions: IndexMap<String, EntityDimensions>,
    #[serde(default)]
    pub metadata: Vec<EntityMetadateEntry>
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct EntityDimensions {
    fixed: bool,
    eye_height: f32,
    width: f32,
    height: f32
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

    // Entity Enum
    write_buffer.push_str("#![allow(warnings, unused, unused_assignments)]\n\n");
    write_buffer.push_str("#[derive(Debug, Clone, Copy, Eq, PartialEq)]\n");
    write_buffer.push_str("#[repr(u8)]\n");
    write_buffer.push_str("pub enum Entity {\n");
    for (entity_name, entity_data) in &entities {
        writeln!(write_buffer, "\t{} = {},", entity_name.to_case(Case::Pascal), entity_data.id)?;
    }
    write_buffer.push_str("}\n");

    // EntityAndMetadata Enum
    write_buffer.push_str("#[derive(Clone)]\n");
    write_buffer.push_str("pub enum EntityAndMetadata {\n");
    for (entity_name, _) in &entities {
        let pascal_name = entity_name.to_case(Case::Pascal);
        writeln!(write_buffer, "\t{}({}Metadata),", pascal_name, pascal_name)?;
    }
    write_buffer.push_str("}\n\n");
    write_buffer.push_str("impl EntityAndMetadata {\n");
    write_buffer.push_str("\tpub fn entity(&self) -> Entity {\n");
    write_buffer.push_str("\t\tmatch self {\n");
    for (entity_name, _) in &entities {
        writeln!(write_buffer, "\t\t\tSelf::{}(_) => Entity::{},", entity_name.to_case(Case::Pascal), entity_name.to_case(Case::Pascal))?;
    }
    write_buffer.push_str("\t\t}\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("\tpub fn get_pose(&self) -> crate::types::Pose {\n");
    write_buffer.push_str("\t\tmatch self {\n");
    for (entity_name, _) in &entities {
        writeln!(write_buffer, "\t\t\tSelf::{}(metadata) => metadata.pose,", entity_name.to_case(Case::Pascal))?;
    }
    write_buffer.push_str("\t\t}\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n\n");

    write_buffer.push_str(r#"
impl Default for EntityAndMetadata {
    fn default() -> Self {
        Self::Marker(MarkerMetadata::default())
    }
}
"#);

    write_buffer.push_str("impl Metadata for EntityAndMetadata {\n");
    write_buffer.push_str("\tfn has_changed(&self) -> bool {\n");
    write_buffer.push_str("\t\tmatch self {\n");
    for (entity_name, _) in &entities {
        writeln!(write_buffer, "\t\t\tSelf::{}(metadata) => metadata.has_changed(),", entity_name.to_case(Case::Pascal))?;
    }
    write_buffer.push_str("\t\t}\n");
    write_buffer.push_str("\t}\n");

    write_buffer.push_str("\tfn clear_all_changes(&mut self) {\n");
    write_buffer.push_str("\t\tmatch self {\n");
    for (entity_name, _) in &entities {
        writeln!(write_buffer, "\t\t\tSelf::{}(metadata) => metadata.clear_all_changes(),", entity_name.to_case(Case::Pascal))?;
    }
    write_buffer.push_str("\t\t}\n");
    write_buffer.push_str("\t}\n");

    write_buffer.push_str("\tfn get_changes_write_size(&self) -> usize {\n");
    write_buffer.push_str("\t\tmatch self {\n");
    for (entity_name, _) in &entities {
        writeln!(write_buffer, "\t\t\tSelf::{}(metadata) => metadata.get_changes_write_size(),", entity_name.to_case(Case::Pascal))?;
    }
    write_buffer.push_str("\t\t}\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("\tunsafe fn write_changes<'b>(&mut self, bytes: &'b mut [u8]) -> &'b mut [u8] {\n");
    write_buffer.push_str("\t\tmatch self {\n");
    for (entity_name, _) in &entities {
        writeln!(write_buffer, "\t\t\tSelf::{}(metadata) => metadata.write_changes(bytes),", entity_name.to_case(Case::Pascal))?;
    }
    write_buffer.push_str("\t\t}\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("\tfn get_non_default_write_size(&self) -> usize {\n");
    write_buffer.push_str("\t\tmatch self {\n");
    for (entity_name, _) in &entities {
        writeln!(write_buffer, "\t\t\tSelf::{}(metadata) => metadata.get_non_default_write_size(),", entity_name.to_case(Case::Pascal))?;
    }
    write_buffer.push_str("\t\t}\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("\tunsafe fn write_non_default<'b>(&self, bytes: &'b mut [u8]) -> &'b mut [u8] {\n");
    write_buffer.push_str("\t\tmatch self {\n");
    for (entity_name, _) in &entities {
        writeln!(write_buffer, "\t\t\tSelf::{}(metadata) => metadata.write_non_default(bytes),", entity_name.to_case(Case::Pascal))?;
    }
    write_buffer.push_str("\t\t}\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");

    // Metadata
    write_buffer.push_str(r#"
#[derive(Debug, thiserror::Error)]
#[error("Invalid metadata changes")]
pub struct InvalidMetadataChanges;
pub trait Metadata: Default {
    fn has_changed(&self) -> bool;
    fn clear_all_changes(&mut self);

    fn get_changes_write_size(&self) -> usize;
    unsafe fn write_changes<'b>(&mut self, bytes: &'b mut [u8]) -> &'b mut [u8];

    fn get_non_default_write_size(&self) -> usize;
    unsafe fn write_non_default<'b>(&self, bytes: &'b mut [u8]) -> &'b mut [u8];
}

#[derive(Default, Clone)]
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
    fn is_changed(&self, index: usize) -> bool {
        match self {
            Self::NoChanges => false,
            Self::SingleChange { index: changed_index } => {
                *changed_index == index
            },
            Self::ManyChanges { indices } => {
                indices[index]
            }
        }
    }

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

    fn unmark_dirty(&mut self, index: usize) {
        match self {
            Self::NoChanges => {
            },
            Self::SingleChange { index: old_index } => {
                if *old_index == index {
                    *self = Self::NoChanges;
                }
            },
            Self::ManyChanges { indices } => {
                indices[index] = false;
            }
        }
    }
}

use graphite_binary::slice_serialization::*;

"#);

    for (entity_name, entity_data) in &entities {
        let pascal_name = entity_name.to_case(Case::Pascal);

        let metadata = &entity_data.metadata;

        let lifetime = String::new();
        // for entry in metadata {
        //     if entry.serializer == "item_stack" {
        //         lifetime.push_str("<'a>");
        //         break;
        //     }
        // }

        writeln!(write_buffer, "#[readonly::make]")?;
        writeln!(write_buffer, "#[derive(Default, Clone)]")?;
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
            
            writeln!(
                write_buffer,
                "\tpub fn unmark_changes_to_{}(&mut self) {{",
                entry.name
            )?;
            writeln!(write_buffer, "\t\tself.changes.unmark_dirty({});", index)?;
            write_buffer.push_str("\t}\n");


            writeln!(
                write_buffer,
                "\tpub fn is_{}_changed(&self) -> bool {{",
                entry.name
            )?;

            writeln!(write_buffer, "\t\tself.changes.is_changed({})", index)?;
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
        fn has_changed(&self) -> bool {
            match self.changes {
                MetadataChanges::NoChanges => false,
                MetadataChanges::SingleChange { index: _ } => true,
                MetadataChanges::ManyChanges { indices } => true,
            }
        }

        fn clear_all_changes(&mut self) {
            self.changes = MetadataChanges::NoChanges;
        }
    "#);

        // Write changes
        write_buffer.push_str(r#"
    fn get_changes_write_size(&self) -> usize {
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
        bytes
    }
"#,
        );

        // Write non-default
        write_buffer.push_str(r#"
    fn get_non_default_write_size(&self) -> usize {
        let mut size = 1;
"#);
        for index in 0..metadata.len() {
            let mut name = metadata[index].name.as_str();
            if name == "type" {
                name = "r#type";
            }

            if metadata[index].serializer == "item_stack" {
                writeln!(
                    write_buffer,
                    "\t\tif self.{}.is_some() {{ size += 2 + self.get_write_size_for_index({}); }}",
                    name, index
                )?;
                continue;
            }

            let mut rust_type = format!("{}::default()", serialize_type_to_rust_type(&metadata[index].serializer));
            if rust_type.contains("<") || rust_type.contains(",") {
                rust_type = "Default::default()".to_owned();
            }
            if rust_type == "()::default()" {
                rust_type = "()".to_owned();
            }

            writeln!(
                write_buffer,
                "\t\tif self.{} != {} {{ size += 2 + self.get_write_size_for_index({}); }}",
                name, rust_type, index
            )?;
        }

        write_buffer.push_str(
            r#"        size
    }

    unsafe fn write_non_default<'b>(&self, mut bytes: &'b mut [u8]) -> &'b mut [u8] {
"#,
        );
        for index in 0..metadata.len() {
            let mut name = metadata[index].name.as_str();
            if name == "type" {
                name = "r#type";
            }

            if metadata[index].serializer == "item_stack" {
                writeln!(
                    write_buffer,
                    "\t\tif self.{}.is_some() {{ bytes = self.write_for_index(bytes, {}); }}",
                    name, index
                )?;
                continue;
            }

            let mut rust_type = format!("{}::default()", serialize_type_to_rust_type(&metadata[index].serializer));
            if rust_type.contains("<") || rust_type.contains(",") {
                rust_type = "Default::default()".to_owned();
            }
            if rust_type == "()::default()" {
                rust_type = "()".to_owned();
            }

            writeln!(
                write_buffer,
                "\t\tif self.{} != {} {{ bytes = self.write_for_index(bytes, {}); }}",
                name, rust_type, index
            )?;
        }

        write_buffer.push_str(
            r#"        bytes = <Single as SliceSerializable<u8>>::write(bytes, 255);
        bytes
    }
"#,
        );

        write_buffer.push_str("}\n\n");

        // break;
    }

    write_buffer.push_str(r#"
#[derive(Clone, Debug)]
pub struct EntityDimensions {
    pub fixed: bool,
    pub eye_height: f32,
    pub width: f32,
    pub height: f32
}

impl EntityDimensions {
    pub fn scale(&self, scale: f32) -> Self {
        if self.fixed {
            self.clone()
        } else {
            Self {
                fixed: false,
                eye_height: self.eye_height * scale,
                width: self.width * scale,
                height: self.height * scale,
            } 
        }
    }
}

impl Entity {
    pub fn get_properties(self) -> &'static EntityProperties {
        &ENTITY_PROPERTIES_LUT[self as usize]
    }

    pub fn get_default_dimensions(self, pose: crate::types::Pose) -> EntityDimensions {
        match self {
"#);
    for (entity_name, entity_data) in &entities {
        let pascal = entity_name.to_case(Case::Pascal);
        writeln!(write_buffer, "\t\t\tSelf::{} => {{", pascal)?;
        write_buffer.push_str("\t\t\t\tmatch pose {\n");
        for (name, dimensions) in &entity_data.dimensions {
            let matcher = if name == "default" {
                "_".to_string()
            } else {
                let pascal_pose = name.to_case(Case::Pascal);
                format!("crate::types::Pose::{}", pascal_pose)
            };

            writeln!(write_buffer, "\t\t\t\t\t{} => EntityDimensions {{", matcher)?;
            writeln!(write_buffer, "\t\t\t\t\t\tfixed: {},", dimensions.fixed)?;
            writeln!(write_buffer, "\t\t\t\t\t\teye_height: {:?}_f32,", dimensions.eye_height)?;
            writeln!(write_buffer, "\t\t\t\t\t\twidth: {:?}_f32,", dimensions.width)?;
            writeln!(write_buffer, "\t\t\t\t\t\theight: {:?}_f32,", dimensions.height)?;
            write_buffer.push_str("\t\t\t\t\t},\n");
            

        }

        write_buffer.push_str("\t\t\t\t}\n");
        write_buffer.push_str("\t\t\t},\n");
    }
    write_buffer.push_str(r#"
        }
    }
}"#);



    write_buffer.push_str("\n\n");

    // Entity Properties Struct
    write_buffer.push_str("#[derive(Debug)]\n");
    write_buffer.push_str("pub struct EntityProperties {\n");
    write_buffer.push_str("\tpub interpolation_duration: usize,\n");
    write_buffer.push_str("\tpub is_living_entity: bool,\n");
    write_buffer.push_str("}\n\n");

    writeln!(
        write_buffer,
        "const ENTITY_PROPERTIES_LUT: [EntityProperties; {}] = [",
        entity_count
    )?;
    for (entity_name, entity) in &entities {
        writeln!(write_buffer, "\tEntityProperties {{ // {}", entity_name)?;
        writeln!(write_buffer, "\t\tinterpolation_duration: {}_usize,", entity.interpolation_duration)?;
        writeln!(write_buffer, "\t\tis_living_entity: {},", entity.is_living_entity)?;
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
            "<Option<NBTBlob> as SliceSerializable<_>>::write(bytes, &self.{varname}.clone())"
        ),
        "item_stack" => {
            format!("if let Some(item_stack) = &self.{varname} {{ <WriteOnlyBlob as SliceSerializable<Box<[u8]>>>::write(bytes, item_stack) }} else {{ bytes[0] = 0; &mut bytes[1..] }}")
        }
        "boolean" => format!("<Single as SliceSerializable<bool>>::write(bytes, self.{varname})"),
        "rotations" => format!(
            "{{
            bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.0);
            bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.1);
            <BigEndian as SliceSerializable<f32>>::write(bytes, self.{varname}.2)
        }}"
        ),
        "block_pos" => format!("<BigEndian as SliceSerializable<i64>>::write(bytes, self.{varname})"),
        "optional_block_pos" => format!("<Option<BigEndian> as SliceSerializable<Option<i64>>>::write(bytes, &self.{varname})"),
        "direction" => "unimplemented!()".into(),
        "optional_living_entity_reference" => "unimplemented!()".into(),
        "block_state" => format!("<VarInt as SliceSerializable<i32>>::write(bytes, self.{varname})"),
        "optional_block_state" => format!("<VarInt as SliceSerializable<i32>>::write(bytes, self.{varname}.unwrap_or(0))"),
        "compound_tag" => "unimplemented!()".into(),
        "particle" => "unimplemented!()".into(),
        "particles" => "unimplemented!()".into(),
        "villager_data" => "unimplemented!()".into(),
        "optional_unsigned_int" => "unimplemented!()".into(),
        "pose" => format!("<Single as SliceSerializable<u8>>::write(bytes, self.{varname} as u8)"),
        "cat_variant" => "unimplemented!()".into(),
        "cow_variant" => "unimplemented!()".into(),
        "wolf_variant" => "unimplemented!()".into(),
        "wolf_sound_variant" => "unimplemented!()".into(),
        "frog_variant" => "unimplemented!()".into(),
        "pig_variant" => "unimplemented!()".into(),
        "chicken_variant" => "unimplemented!()".into(),
        "optional_global_pos" => "unimplemented!()".into(),
        "painting_variant" => "unimplemented!()".into(),
        "sniffer_state" => "unimplemented!()".into(),
        "armadillo_state" => "unimplemented!()".into(),
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
        "optional_living_entity_reference" => 13,
        "block_state" => 14,
        "optional_block_state" => 15,
        "compound_tag" => 16,
        "particle" => 17,
        "particles" => 18,
        "villager_data" => 19,
        "optional_unsigned_int" => 20,
        "pose" => 21,
        "cat_variant" => 22,
        "cow_variant" => 23,
        "wolf_variant" => 24,
        "wolf_sound_variant" => 25,
        "frog_variant" => 26,
        "pig_variant" => 27,
        "chicken_variant" => 28,
        "optional_global_pos" => 29,
        "painting_variant" => 30,
        "sniffer_state" => 31,
        "armadillo_state" => 32,
        "vector3" => 33,
        "quaternion" => 34,
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
            format!("if let Some(item_stack) = &self.{varname} {{ item_stack.len() }} else {{ 1 }}")
        }
        "boolean" => "1".into(),
        "rotations" => "12".into(),
        "block_pos" => "8".into(),
        "optional_block_pos" => format!("1 + if self.{varname}.is_some() {{ 8 }} else {{ 0 }}"),
        "direction" => "1".into(),
        "optional_living_entity_reference" => format!("1 + if self.{varname}.is_some() {{ 16 }} else {{ 0 }}"),
        "block_state" => "5".into(),
        "optional_block_state" => "5".into(),
        "compound_tag" => "unimplemented!()".into(),
        "particle" => "unimplemented!()".into(),
        "particles" => "unimplemented!()".into(),
        "villager_data" => "7".into(), // todo: add data type in protocol
        "optional_unsigned_int" => "5".into(),
        "pose" => "1".into(),
        "cat_variant" => "1".into(),
        "cow_variant" => "1".into(),
        "wolf_variant" => "1".into(),
        "wolf_sound_variant" => "1".into(),
        "frog_variant" => "1".into(),
        "pig_variant" => "1".into(),
        "chicken_variant" => "1".into(),
        "optional_global_pos" => format!(
            "1 + if let Some((world, _)) = &self.{varname} {{ 5 + world.len() + 8 }} else {{ 8 }}"
        ),
        "painting_variant" => "1".into(),
        "sniffer_state" => "unimplemented!()".into(),
        "armadillo_state" => "unimplemented!()".into(),
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
        "component" => "graphite_binary::nbt::EncodedNBT",
        "optional_component" => "Option<graphite_binary::nbt::EncodedNBT>",
        "item_stack" => "Option<Box<[u8]>>",
        "boolean" => "bool",
        "rotations" => "(f32, f32, f32)",
        "block_pos" => "i64",
        "optional_block_pos" => "Option<i64>",
        "direction" => "crate::types::Direction",
        "optional_living_entity_reference" => "Option<u128>",
        "block_state" => "i32",
        "optional_block_state" => "Option<i32>",
        "compound_tag" => "()",
        "particle" => "()",
        "particles" => "()",
        "villager_data" => "(u8, u8, i32)", // todo: add data type in protocol
        "optional_unsigned_int" => "Option<u32>",
        "pose" => "crate::types::Pose",
        "cat_variant" => "u8",
        "cow_variant" => "u8",
        "wolf_variant" => "u8",
        "wolf_sound_variant" => "u8",
        "frog_variant" => "u8",
        "pig_variant" => "u8",
        "chicken_variant" => "u8",
        "optional_global_pos" => "Option<(String, i64)>",
        "painting_variant" => "u8",
        "sniffer_state" => "()",
        "armadillo_state" => "()",
        "vector3" => "(f32, f32, f32)",
        "quaternion" => "(f32, f32, f32, f32)",
        _ => panic!("unknown serialize type: {}", typ),
    }
}
