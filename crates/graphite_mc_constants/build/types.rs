use std::{fmt::Write as _, i32};
use std::io::Write;

use convert_case::{Case, Casing};
use indexmap::IndexMap;

#[derive(serde_derive::Deserialize)]
#[serde(untagged)]
pub enum Type {
    List(Vec<String>),
    Numbered(IndexMap<String, i32>)
}

pub fn write_types() -> anyhow::Result<()> {
    let raw_data = include_str!("../data/types.json");
    let enums: IndexMap<String, Type> = serde_json::from_str(raw_data)?;

    let mut write_buffer = String::new();

    for (name, values) in enums {
        let pretty_name = name.to_case(Case::Pascal);

        let repr = if pretty_name == "LevelEvent" {
            "i32"
        } else {
            find_best_repr(&values)
        };

        // Enum
        let mut write_default = true;
        if pretty_name == "RelativeMovement" {
            write_buffer.push_str("#[derive(Debug, enumset::EnumSetType)]\n");
            write_default = false;
        } else {
            write_buffer.push_str("#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, strum_macros::IntoStaticStr, num_enum::TryFromPrimitive, num_enum::IntoPrimitive)]\n");
            write_buffer.push_str("#[strum(serialize_all = \"snake_case\")]\n");
        }
        write_buffer.push_str(&format!("#[repr({})]\n", repr));
        write_buffer.push_str(&format!("pub enum {} {{\n", pretty_name));
        match values {
            Type::List(vec) => {
                for (index, value) in vec.iter().enumerate() {
                    if index == 0 && write_default {
                        write_buffer.push_str("\t#[default]\n");
                    }
                    writeln!(write_buffer, "\t{} = {},", value.replace(".", "_").replace("/", "_").to_case(Case::Pascal), index)?;
                }
            },
            Type::Numbered(index_map) => {
                for (value, index) in index_map {
                    if write_default {
                        write_buffer.push_str("\t#[default]\n");
                        write_default = false;
                    }
                    writeln!(write_buffer, "\t{} = {},", value.replace(".", "_").replace("/", "_").to_case(Case::Pascal), index)?;
                }
            },
        }
        write_buffer.push_str("}\n\n");

        if pretty_name == "RelativeMovement" {
            write_buffer.push_str(&format!("impl {} {{\n", pretty_name));
            write_buffer.push_str("\tpub const POSITION: enumset::EnumSet<Self> = enumset::enum_set_union!(Self::X, Self::Y, Self::Z);\n");
            write_buffer.push_str("\tpub const ROTATION: enumset::EnumSet<Self> = enumset::enum_set_union!(Self::XRot, Self::YRot);\n");
            write_buffer.push_str("\tpub const VELOCITY: enumset::EnumSet<Self> = enumset::enum_set_union!(Self::DeltaX, Self::DeltaY, Self::DeltaZ);\n");
            write_buffer.push_str("}\n\n");
        }
    }

    let mut f = crate::file_src("types.rs");
    f.write_all(write_buffer.as_bytes())?;

    Ok(())
}

fn find_best_repr(values: &Type) -> &str {
    let mut min = i32::MAX;
    let mut max = i32::MIN;

    match values {
        Type::List(vec) => {
            min = 0;
            max = (vec.len() - 1) as i32;
        },
        Type::Numbered(index_map) => {
            for &index in index_map.values() {
                min = min.min(index);
                max = max.max(index);
            }
        },
    }

    let absolute = min.abs().max(max.abs());
    let repr = if min >= 0 {
        if absolute <= u8::MAX as i32 {
            "u8"
        } else if absolute <= u16::MAX as i32 {
            "u16"
        } else if absolute <= u32::MAX as i32 {
            "u32"
        } else {
            "i32"
        }
    } else {
        if absolute <= i8::MAX as i32 {
            "i8"
        } else if absolute <= i16::MAX as i32 {
            "i16"
        } else {
            "i32"
        }
    };
    repr
}