use std::{collections::HashMap, fmt::Write as _};
use std::io::Write;

use anyhow::bail;
use convert_case::{Case, Casing};
use indexmap::IndexMap;

#[derive(serde_derive::Deserialize)]
#[serde(untagged)]
enum Primitive {
    Index(usize),
    Integer(i32),
    Double(f64),
    String(String),
    Boolean(bool)
}

impl Primitive {
    pub fn to_type(&self) -> PrimitiveType {
        match self {
            Primitive::Index(_) => PrimitiveType::Index,
            Primitive::Integer(_) => PrimitiveType::Integer,
            Primitive::Double(_) => PrimitiveType::Double,
            Primitive::String(_) => PrimitiveType::String,
            Primitive::Boolean(_) => PrimitiveType::Boolean,
        }
    }

    pub fn to_literal(&self) -> String {
        match self {
            Primitive::Index(value) => value.to_string(),
            Primitive::Integer(value) => value.to_string(),
            Primitive::Double(value) => format!("{:?}_f64", value),
            Primitive::String(value) => format!("\"{}\"", value),
            Primitive::Boolean(value) => value.to_string(),
        }
    }
}

#[derive(Debug, PartialEq)]
enum PrimitiveType {
    Index,
    Integer,
    Double,
    String,
    Boolean
}

struct RegistryValueType {
    value_type: PrimitiveType,
    count: usize
}

#[derive(serde_derive::Deserialize)]
struct RegistryValues(IndexMap<String, Primitive>);

pub struct BuiltinResult {
    pub data_component_types: Vec<String>
}

pub fn write_registries() -> anyhow::Result<BuiltinResult> {
    let raw_data = include_str!("../data/builtin.json");
    let built_in_registries: IndexMap<String, IndexMap<String, RegistryValues>> = serde_json::from_str(raw_data)?;

    let mut data_component_types = Vec::new();

    let mut write_buffer = String::new();

    for (registry_name, registry) in built_in_registries {
        if registry.is_empty() {
            continue;
        }

        if registry_name == "data_component_type" {
            for data_component_type in registry.keys() {
                data_component_types.push(data_component_type.to_string());
            }
        }

        let mut pretty_name = registry_name.replace(".", "_").replace("/", "_").replace(":", "_").to_case(Case::Pascal);

        if pretty_name == "Menu" {
            pretty_name = "ContainerType".to_owned();
        }

        let mut combined_registry_values: IndexMap<String, RegistryValueType> = IndexMap::new();

        // Enum
        write_buffer.push_str("#[derive(Debug, enumset::EnumSetType, enum_map::Enum, strum_macros::IntoStaticStr, strum_macros::EnumIter, num_enum::TryFromPrimitive, num_enum::IntoPrimitive)]\n");
        if registry.len() <= u8::MAX as usize {
            write_buffer.push_str("#[repr(u8)]\n");
        } else {
            write_buffer.push_str("#[repr(u16)]\n");
        }
        write_buffer.push_str("#[strum(serialize_all = \"snake_case\")]\n");
        write_buffer.push_str(&format!("pub enum {} {{\n", pretty_name));
        for (index, (key, values)) in registry.iter().enumerate() {
            let enum_name = key.replace(".", "_").replace("/", "_").replace(":", "_").to_case(Case::Pascal);
            writeln!(write_buffer, "\t{} = {},", enum_name, index)?;

            for (value_key, primitive) in &values.0 {
                let primitive_type = primitive.to_type();

                if let Some(existing_value) = combined_registry_values.get_mut(value_key) {
                    if existing_value.value_type == primitive_type || existing_value.value_type == PrimitiveType::Integer && primitive_type == PrimitiveType::Index {
                        existing_value.count += 1;
                    } else if existing_value.value_type == PrimitiveType::Index && primitive_type == PrimitiveType::Integer {
                        // Convert index to integer
                        existing_value.value_type = PrimitiveType::Integer;
                        existing_value.count += 1;
                    } else {
                        bail!("Registry {} has value {} with different types {:?} and {:?}", pretty_name,
                            value_key, existing_value.value_type, primitive_type);
                    }
                } else {
                    combined_registry_values.insert(value_key.clone(), RegistryValueType {
                        value_type: primitive_type,
                        count: 1,
                    });
                }
            }
        }
        write_buffer.push_str("}\n\n");


        // Count
        write_buffer.push_str(&format!("impl {} {{\n", pretty_name));
        write_buffer.push_str(&format!("\tpub const COUNT: usize = {};\n", registry.len()));

        for (name, registry_value_type) in combined_registry_values {
            let mut rust_type = match registry_value_type.value_type {
                PrimitiveType::Index => "usize",
                PrimitiveType::Integer => "i32",
                PrimitiveType::Double => "f64",
                PrimitiveType::String => "&'static str",
                PrimitiveType::Boolean => "bool",
            }.to_string();
            let optional = registry_value_type.count < registry.len();
            if optional {
                rust_type = format!("Option<{}>", rust_type);
            }

            write_buffer.push_str(&format!("\n\tpub const fn {}(self) -> {} {{\n", name, rust_type));
            write_buffer.push_str(&format!("\t\tmatch self {{\n"));
            for (key, values) in &registry {
                let enum_name = key.replace(".", "_").replace("/", "_").replace(":", "_").to_case(Case::Pascal);
                let value = values.0.get(&name);

                if let Some(value) = value {
                    if optional {
                        write_buffer.push_str(&format!("\t\t\tSelf::{} => Some({}),\n", enum_name, value.to_literal()));
                    } else {
                        write_buffer.push_str(&format!("\t\t\tSelf::{} => {},\n", enum_name, value.to_literal()));
                    }
                } else {
                    write_buffer.push_str(&format!("\t\t\tSelf::{} => None,", enum_name));
                }
            }
            write_buffer.push_str("\t\t}\n");
            write_buffer.push_str("\t}\n");
        }
        write_buffer.push_str("}\n\n");
    }

    let mut f = crate::file_src("builtin.rs");
    f.write_all(write_buffer.as_bytes())?;

    Ok(BuiltinResult { data_component_types })
}