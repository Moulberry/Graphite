use std::fmt::Write as _;
use std::{collections::HashMap, io::Write};

use anyhow::bail;
use convert_case::{Case, Casing};
use indexmap::IndexMap;
use serde_derive::Deserialize;

use crate::file_src;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    #[serde(default = "IndexMap::new")]
    properties: IndexMap<String, BlockParameter>,
    hardness: f32,
    #[serde(default)]
    air: bool
    
    /*name: &'static str,
    display_name: &'static str,
    hardness: f32,
    resistance: f32,
    stack_size: u8,
    diggable: bool,
    material: &'static str,
    transparent: bool,
    emit_light: u8,
    filter_light: u8,
    default_state: u16,
    min_state_id: u16,
    max_state_id: u16,
    states: Vec<BlockParameter>,
    bounding_box: &'static str,*/
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum BlockParameter {
    #[serde(rename = "string")]
    Enum {
        values: Vec<String>,
    },
    #[serde(rename = "bool")]
    Bool {
    },
    #[serde(rename = "int")]
    Int {
        values: Vec<i32>,
    },
}

pub fn write_block_states() -> anyhow::Result<()> {
    let raw_data = include_str!("../data/blocks.json");
    let blocks: IndexMap<String, Block> = serde_json::from_str(raw_data)?;

    // Codegen all the parameters
    let mut parameter_writer: ParameterWriter = Default::default();
    for (_, block) in &blocks {
        for (name, parameter) in &block.properties {
            if let BlockParameter::Enum {
                values,
            } = parameter {
                parameter_writer.define_parameter(name, values)?;
            }
        }
    }

    let mut block_def = String::new();
    let mut u16_from_block_def = String::new();
    let mut state_lut = String::new();
    let mut state_properties_lut = String::new();
    let mut state_count = 0;
    for (block_name, block) in &blocks {
        let min_state_count = state_count;
        state_count += write_block_state(
            &mut block_def,
            &mut state_lut,
            &mut u16_from_block_def,
            &parameter_writer,
            block_name,
            block,
            state_count
        )?;

        // Block Properties
        for _state_id in min_state_count..state_count {
            // todo: allow overriding properties for a particular state
            writeln!(state_properties_lut, "\tBlockProperties {{ // {}", block_name)?;
            writeln!(state_properties_lut, "\t\thardness: {}_f32,", block.hardness)?;
            writeln!(state_properties_lut, "\t\tair: {},", block.air)?;
            state_properties_lut.push_str("\t},\n");
        }
    }

    let mut write_buffer = String::new();

    // Block Into<u16>
    write_buffer.push_str("impl From<&Block> for u16 {\n");
    write_buffer.push_str("\tfn from(block: &Block) -> u16 {\n");
    write_buffer.push_str("\t\tmatch block {\n");
    write_buffer.push_str(&u16_from_block_def);
    write_buffer.push_str("\t\t_ => 0\n");
    write_buffer.push_str("\t\t}\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    let mut f = crate::file_out("block_to_u16.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();

    // Block TryFrom<u16> + LUT
    write_buffer.push_str("impl TryFrom<u16> for &Block {\n");
    write_buffer.push_str("\ttype Error = NoSuchBlockError;");
    write_buffer.push_str("\tfn try_from(id: u16) -> Result<&'static Block, Self::Error> {\n");
    write_buffer
        .push_str("\t\tif id >= BLOCK_LUT.len() as _ { return Err(NoSuchBlockError(id)); }\n");
    write_buffer.push_str("\t\tOk(&BLOCK_LUT[id as usize])\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    writeln!(
        write_buffer,
        "const BLOCK_LUT: [Block; {}] = [",
        state_count
    )?;
    write_buffer.push_str(&state_lut);
    write_buffer.push_str("];");
    let mut f = crate::file_out("u16_to_block.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();

    // Block Properties
    write_buffer.push_str("impl TryFrom<u16> for &BlockProperties {\n");
    write_buffer.push_str("\ttype Error = NoSuchBlockError;");
    write_buffer.push_str("\tfn try_from(id: u16) -> Result<&'static BlockProperties, Self::Error> {\n");
    write_buffer
        .push_str("\t\tif id >= BLOCK_PROPERTIES_LUT.len() as _ { return Err(NoSuchBlockError(id)); }\n");
    write_buffer.push_str("\t\tOk(&BLOCK_PROPERTIES_LUT[id as usize])\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    writeln!(
        write_buffer,
        "const BLOCK_PROPERTIES_LUT: [BlockProperties; {}] = [",
        state_count
    )?;
    write_buffer.push_str(&state_properties_lut);
    write_buffer.push_str("];");
    let mut f = crate::file_out("u16_to_block_properties.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();

    // Block Parameters
    let mut f = file_src("block_parameter.rs");
    f.write_all(parameter_writer.get_enum_code().as_bytes())?;
    write_buffer.clear();

    // Block enum
    write_buffer.push_str("use crate::block_parameter::*;\n\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/block_to_u16.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/u16_to_block.rs\"));\n\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/u16_to_block_properties.rs\"));\n\n");

    write_buffer.push_str("#[derive(Debug, thiserror::Error)]\n");
    write_buffer.push_str("#[error(\"No block exists for id: {0}\")]\n");
    write_buffer.push_str("pub struct NoSuchBlockError(u16);\n\n");

    write_buffer.push_str("#[derive(Debug)]\npub enum Block {\n");
    write_buffer.push_str(&block_def);
    write_buffer.push_str("}\n\n");

    // Block Properties Struct
    write_buffer.push_str("#[derive(Debug)]\n");
    write_buffer.push_str("pub struct BlockProperties {\n");
    write_buffer.push_str("\tpub hardness: f32,\n");
    write_buffer.push_str("\tpub air: bool,\n");
    write_buffer.push_str("}\n\n");

    let mut f = crate::file_src("block.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();

    Ok(())
}

/*impl TryFrom<u16> for u8 {
    type Error;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
    }
}*/

fn write_block_state(
    block_def: &mut String,
    state_lut: &mut String,
    u16_from_block_def: &mut String,
    parameters: &ParameterWriter,
    block_name: &str,
    block: &Block,
    current_state_id: usize,
) -> anyhow::Result<usize> {
    let mut all_possible_parameters = Vec::new();

    block_def.push('\t');
    block_def.push_str(&block_name.to_case(Case::Pascal));

    if block.properties.is_empty() {
        block_def.push_str(",\n");

        u16_from_block_def.push_str("\t\t\tBlock::");
        u16_from_block_def.push_str(&block_name.to_case(Case::Pascal));
        writeln!(u16_from_block_def, " => {},", current_state_id)?;

        state_lut.push_str("\tBlock::");
        state_lut.push_str(&block_name.to_case(Case::Pascal));
        state_lut.push_str(",\n");

        return Ok(1);
    } else {
        block_def.push_str(" {\n");

        for (name, state) in &block.properties {
            match state {
                BlockParameter::Enum {
                    values,
                } => {
                    let parameter_name = parameters.get_parameter_name(name, values);

                    if *name == "type" {
                        writeln!(block_def, "\t\tblock_type: {parameter_name},")?;

                        let mut named_values = Vec::new();
                        for value in values {
                            let value = value.to_case(Case::Pascal);
                            named_values.push(format!("block_type: {parameter_name}::{},", value));
                        }
                        all_possible_parameters.push(named_values);
                    } else {
                        writeln!(block_def, "\t\t{name}: {parameter_name},")?;

                        let mut named_values = Vec::new();
                        for value in values {
                            let value = value.to_case(Case::Pascal);
                            named_values.push(format!("{name}: {parameter_name}::{},", value));
                        }
                        all_possible_parameters.push(named_values);
                    }
                }
                BlockParameter::Bool {  } => {
                    let mut named_values = Vec::new();
                    named_values.push(format!("{name}: true,"));
                    named_values.push(format!("{name}: false,"));
                    all_possible_parameters.push(named_values);

                    block_def.push_str("\t\t");
                    block_def.push_str(name);
                    block_def.push_str(": bool,\n");
                }
                BlockParameter::Int {
                    values,
                } => {
                    block_def.push_str("\t\t");
                    block_def.push_str(name);
                    block_def.push_str(": u8,\n");

                    let mut named_values = Vec::new();
                    for value in values {
                        named_values.push(format!("{name}: {value},"));
                    }
                    all_possible_parameters.push(named_values);
                }
            }
        }
        block_def.push_str("\t},\n");
    }

    assert!(!all_possible_parameters.is_empty());

    let mut all: Vec<String> = Vec::new();
    for possible_parameterizations in all_possible_parameters {
        let mut new_all = Vec::new();
        if all.is_empty() {
            for possible_parameterization in &possible_parameterizations {
                new_all.push(possible_parameterization.clone())
            }
        } else {
            for current in &all {
                for possible_parameterization in &possible_parameterizations {
                    let mut current = current.clone();
                    current.push_str(possible_parameterization);
                    new_all.push(current)
                }
            }
        }
        all = new_all;
    }

    let all_count = all.len();

    let mut index = current_state_id;
    for one in all {
        let mut state_def = String::new();
        state_def.push_str("\tBlock::");
        state_def.push_str(&block_name.to_case(Case::Pascal));
        state_def.push('{');
        state_def.push_str(&one);
        state_def.push('}');

        // Push into LUT
        state_lut.push_str(&state_def);
        state_lut.push_str(",\n");

        // Push into From
        writeln!(u16_from_block_def, "\t\t{state_def} => {index},")?;

        index += 1;
    }

    Ok(all_count)
}

#[derive(Default)]
struct ParameterWriter {
    already_aliased: HashMap<String, Vec<Vec<String>>>,
    definitions: HashMap<String, Vec<String>>,
    aliases: HashMap<(String, Vec<String>), String>,
    code: HashMap<String, String>,
}

impl ParameterWriter {
    fn define_parameter(
        &mut self,
        name: &String,
        values: &Vec<String>,
    ) -> anyhow::Result<()> {

        if let Some(previous_aliases) = self.already_aliased.get_mut(name) {
            for previous_alias_value in previous_aliases.iter() {
                if previous_alias_value == values {
                    // Already defined, no need to do anything
                    return Ok(());
                }
            }

            let alias = Self::resolve_clash(name, &values)?;
            previous_aliases.push(values.clone());
            self.code.insert(alias.clone(), Self::codegen(values));
            self.aliases.insert((name.clone(), values.clone()), alias);
            return Ok(());
        }

        if let Some(defined) = self.definitions.get(name) {
            if defined == values {
                // Already defined, no need to do anything
                Ok(())
            } else {
                // Already defined, but with different values... need to alias

                let mut alias_values = Vec::new();

                // Move previous definition
                let previous_code = self.code.remove(name).unwrap();
                let alias = Self::resolve_clash(name, defined)?;
                self.code.insert(alias.clone(), previous_code);
                self.aliases.insert((name.clone(), defined.clone()), alias);
                alias_values.push(defined.clone());

                // Write new definition
                let alias = Self::resolve_clash(name, &values)?;
                self.code.insert(alias.clone(), Self::codegen(&values));
                self.aliases.insert((name.clone(), values.clone()), alias);
                alias_values.push(values.clone());

                // Insert already aliased
                self.already_aliased.insert(name.clone(), alias_values);

                Ok(())
            }
        } else {
            self.code.insert(String::from(name), Self::codegen(&values));
            self.definitions.insert(name.clone(), values.clone());
            Ok(())
        }
    }

    fn resolve_clash(name: &str, values: &Vec<String>) -> anyhow::Result<String> {
        let values: Vec<&'static str> = values.iter().map(|f| unsafe { std::mem::transmute(f.as_str()) }).collect();

        match name {
            "facing" => match values.as_slice() {
                ["north", "east", "south", "west", "up", "down"] => {
                    return Ok(String::from("Facing"))
                }
                ["down", "north", "south", "west", "east"] => {
                    return Ok(String::from("DirectionOrDown"))
                }
                ["north", "south", "west", "east"] => return Ok(String::from("Direction")),
                _ => {}
            },
            "half" => match values.as_slice() {
                ["top", "bottom"] => return Ok(String::from("Half")),
                ["upper", "lower"] => return Ok(String::from("UpperOrLower")),
                _ => {}
            },
            "shape" => match values.as_slice() {
                ["north_south", "east_west", "ascending_east", "ascending_west", "ascending_north", "ascending_south", "south_east", "south_west", "north_west", "north_east"] => {
                    return Ok(String::from("RailShape"))
                }
                ["north_south", "east_west", "ascending_east", "ascending_west", "ascending_north", "ascending_south"] => {
                    return Ok(String::from("StraightRailShape"))
                }
                ["straight", "inner_left", "inner_right", "outer_left", "outer_right"] => {
                    return Ok(String::from("StairShape"))
                }
                _ => {}
            },
            "type" => match values.as_slice() {
                ["normal", "sticky"] => return Ok(String::from("PistonType")),
                ["single", "left", "right"] => return Ok(String::from("ChestType")),
                ["top", "bottom", "double"] => return Ok(String::from("SlabType")),
                _ => {}
            },
            "axis" => match values.as_slice() {
                ["x", "y", "z"] => return Ok(String::from("Axis3D")),
                ["x", "z"] => return Ok(String::from("Axis2D")),
                _ => {}
            },
            "mode" => match values.as_slice() {
                ["compare", "subtract"] => return Ok(String::from("ComparatorMode")),
                ["save", "load", "corner", "data"] => {
                    return Ok(String::from("StructureBlockMode"))
                }
                _ => {}
            },
            "north" => match values.as_slice() {
                ["up", "side", "none"] => return Ok(String::from("WireConnection")),
                ["none", "low", "tall"] => return Ok(String::from("WallConnection")),
                _ => {}
            },
            "east" => match values.as_slice() {
                ["up", "side", "none"] => return Ok(String::from("WireConnection")),
                ["none", "low", "tall"] => return Ok(String::from("WallConnection")),
                _ => {}
            },
            "south" => match values.as_slice() {
                ["up", "side", "none"] => return Ok(String::from("WireConnection")),
                ["none", "low", "tall"] => return Ok(String::from("WallConnection")),
                _ => {}
            },
            "west" => match values.as_slice() {
                ["up", "side", "none"] => return Ok(String::from("WireConnection")),
                ["none", "low", "tall"] => return Ok(String::from("WallConnection")),
                _ => {}
            },
            _ => {}
        }
        bail!(
            "missing aliasing strategy for `{}` with values `{:?}`",
            name,
            values
        )
    }

    fn codegen(values: &Vec<String>) -> String {
        let mut code = String::new();
        code.push_str(" {\n");
        for value in values {
            code.push('\t');
            code.push_str(&value.to_case(Case::Pascal));
            code.push_str(",\n");
        }
        code.push_str("}\n\n");
        code
    }

    fn get_enum_code(&self) -> String {
        let mut code = String::new();
        for (enum_name, enum_def) in self.code.iter() {
            code.push_str("#[repr(u8)]\n#[derive(Debug)]\npub enum ");
            code.push_str(&enum_name.to_case(Case::Pascal));
            code.push_str(enum_def);
        }
        code
    }

    fn get_parameter_name(&self, name: &String, values: &[String]) -> String {
        if let Some(name) = self.aliases.get(&(name.clone(), values.to_owned())) {
            name.clone()
        } else {
            name.to_case(Case::Pascal)
        }
    }
}
