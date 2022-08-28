use std::fmt::Write as _;
use std::io::Write;

use anyhow::bail;
use convert_case::{Case, Casing};
use indexmap::IndexMap;
use serde_derive::Deserialize;

use crate::file_src;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    #[serde(default = "IndexMap::new")]
    pub properties: IndexMap<String, Property>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_placer: Option<CustomPlacer>,
    #[serde(default = "Vec::new")]
    pub placement_conditions: Vec<String>,
    #[serde(default)]
    has_interaction: bool,
    #[serde(default)]
    corresponding_item: String,
    attributes: BlockAttributes,
    #[serde(default = "IndexMap::new")]
    state_attributes: IndexMap<String, BlockAttributes>
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlockAttributes {
    #[serde(default)]
    hardness: Option<f32>,
    #[serde(default)]
    replaceable: Option<bool>,
    #[serde(default)]
    air: Option<bool>,
    #[serde(default)]
    is_north_face_sturdy: Option<bool>,
    #[serde(default)]
    is_east_face_sturdy: Option<bool>,
    #[serde(default)]
    is_south_face_sturdy: Option<bool>,
    #[serde(default)]
    is_west_face_sturdy: Option<bool>
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Property {
    #[serde(rename_all = "camelCase")]
    Int {
        values: Vec<i32>,
        default_value: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        placement_value: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placement_method: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Bool {
        default_value: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        placement_value: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placement_method: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    String {
        values: Vec<String>,
        default_value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        placement_value: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placement_method: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
pub struct CustomPlacer {
    pub name: String,
    pub block: String,
}

pub fn write_block_states() -> anyhow::Result<(
    IndexMap<String, String>,
    IndexMap<(String, Vec<String>), String>,
)> {
    let raw_data = include_str!("../data/blocks.json");
    let blocks: IndexMap<String, Block> = serde_json::from_str(raw_data)?;

    // Codegen all the parameters
    let mut parameter_writer: ParameterWriter = Default::default();
    for (_, block) in &blocks {
        for (name, parameter) in &block.properties {
            if let Property::String {
                values,
                default_value: _,
                placement_value: _,
                placement_method,
            } = parameter
            {
                parameter_writer.define_parameter(name, values, placement_method.clone())?;
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
            state_count,
        )?;

        // Block Properties
        for state_id in min_state_count..state_count {
            // todo: allow overriding properties for a particular state
            writeln!(
                state_properties_lut,
                "\tBlockProperties {{ // {}",
                block_name
            )?;

            let mut hardness = block.attributes.hardness.unwrap_or(0.0);
            let mut replaceable = block.attributes.replaceable.unwrap_or(false);
            let mut air = block.attributes.air.unwrap_or(false);
            let mut is_north_face_sturdy = block.attributes.is_north_face_sturdy.unwrap_or(true);
            let mut is_east_face_sturdy = block.attributes.is_east_face_sturdy.unwrap_or(true);
            let mut is_south_face_sturdy = block.attributes.is_south_face_sturdy.unwrap_or(true);
            let mut is_west_face_sturdy = block.attributes.is_west_face_sturdy.unwrap_or(true);

            let state_attributes = block.state_attributes.get(&state_id.to_string());
            if let Some(state_attributes) = state_attributes {
                hardness = state_attributes.hardness.unwrap_or(hardness);
                replaceable = state_attributes.replaceable.unwrap_or(replaceable);
                air = state_attributes.air.unwrap_or(air);
                is_north_face_sturdy = state_attributes.is_north_face_sturdy.unwrap_or(is_north_face_sturdy);
                is_east_face_sturdy = state_attributes.is_east_face_sturdy.unwrap_or(is_east_face_sturdy);
                is_south_face_sturdy = state_attributes.is_south_face_sturdy.unwrap_or(is_south_face_sturdy);
                is_west_face_sturdy = state_attributes.is_west_face_sturdy.unwrap_or(is_west_face_sturdy);
            }

            writeln!(state_properties_lut, "\t\thardness: {}_f32,", hardness)?;
            writeln!(state_properties_lut, "\t\treplaceable: {},", replaceable)?;
            writeln!(state_properties_lut, "\t\tair: {},", air)?;
            writeln!(state_properties_lut, "\t\tis_north_face_sturdy: {},", is_north_face_sturdy)?;
            writeln!(state_properties_lut, "\t\tis_east_face_sturdy: {},", is_east_face_sturdy)?;
            writeln!(state_properties_lut, "\t\tis_south_face_sturdy: {},", is_south_face_sturdy)?;
            writeln!(state_properties_lut, "\t\tis_west_face_sturdy: {},", is_west_face_sturdy)?;

            state_properties_lut.push_str("\t},\n");
        }
    }

    let mut write_buffer = String::new();

    // Block Into<u16>
    write_buffer.push_str("impl From<&Block> for u16 {\n");
    write_buffer.push_str("\tfn from(block: &Block) -> u16 {\n");
    write_buffer.push_str("\t\tblock.to_id()\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");

    write_buffer.push_str("impl Block {\n");
    write_buffer.push_str("\tpub const fn to_id(&self) -> u16 {\n");
    write_buffer.push_str("\t\tmatch self {\n");
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
    write_buffer
        .push_str("\tfn try_from(id: u16) -> Result<&'static BlockProperties, Self::Error> {\n");
    write_buffer.push_str(
        "\t\tif id >= BLOCK_PROPERTIES_LUT.len() as _ { return Err(NoSuchBlockError(id)); }\n",
    );
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
    write_buffer
        .push_str("include!(concat!(env!(\"OUT_DIR\"), \"/u16_to_block_properties.rs\"));\n\n");

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
    write_buffer.push_str("\tpub replaceable: bool,\n");
    write_buffer.push_str("\tpub air: bool,\n");
    write_buffer.push_str("\tpub is_north_face_sturdy: bool,\n");
    write_buffer.push_str("\tpub is_east_face_sturdy: bool,\n");
    write_buffer.push_str("\tpub is_south_face_sturdy: bool,\n");
    write_buffer.push_str("\tpub is_west_face_sturdy: bool,\n");
    write_buffer.push_str("}\n\n");

    let mut f = crate::file_src("block.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();

    Ok((
        parameter_writer.get_placement_method_returns().clone(),
        parameter_writer.get_aliases().clone(),
    ))
}

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
                Property::String {
                    values,
                    default_value: _,
                    placement_value: _,
                    placement_method: _,
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
                Property::Bool {
                    default_value: _,
                    placement_value: _,
                    placement_method: _,
                } => {
                    let mut named_values = Vec::new();
                    named_values.push(format!("{name}: true,"));
                    named_values.push(format!("{name}: false,"));
                    all_possible_parameters.push(named_values);

                    block_def.push_str("\t\t");
                    block_def.push_str(name);
                    block_def.push_str(": bool,\n");
                }
                Property::Int {
                    values,
                    default_value: _,
                    placement_value: _,
                    placement_method: _,
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
    already_aliased: IndexMap<String, Vec<(String, Vec<String>)>>,
    definitions: IndexMap<String, Vec<String>>,
    aliases: IndexMap<(String, Vec<String>), String>,
    code: IndexMap<String, String>,

    placement_method_returns: IndexMap<String, String>,
}

impl ParameterWriter {
    fn define_parameter(
        &mut self,
        name: &String,
        values: &Vec<String>,
        placement_method: Option<String>,
    ) -> anyhow::Result<()> {
        if let Some(previous_aliases) = self.already_aliased.get_mut(name) {
            for (alias, previous_alias_value) in previous_aliases.iter() {
                if previous_alias_value == values {
                    // Already defined, no need to do anything
                    if let Some(placement_method) = placement_method {
                        let old = self
                            .placement_method_returns
                            .insert(placement_method.clone(), alias.clone());
                        if let Some(old) = old {
                            if old.as_str() != alias.as_str() {
                                bail!(
                                    "Duplicate placement method `{}`, for both {} and {} (1)",
                                    placement_method,
                                    old,
                                    alias
                                )
                            }
                        }
                    }
                    return Ok(());
                }
            }

            let alias = Self::resolve_clash(name, values)?;
            previous_aliases.push((alias.clone(), values.clone()));
            self.code.insert(alias.clone(), Self::codegen(values));
            self.aliases
                .insert((name.clone(), values.clone()), alias.clone());
            if let Some(placement_method) = placement_method {
                let old = self
                    .placement_method_returns
                    .insert(placement_method.clone(), alias.clone());
                if let Some(old) = old {
                    if old.as_str() != alias.as_str() {
                        bail!(
                            "Duplicate placement method `{}`, for both {} and {} (2)",
                            placement_method,
                            old,
                            alias
                        )
                    }
                }
            }
            return Ok(());
        }

        if let Some(defined) = self.definitions.get(name) {
            if defined == values {
                // Already defined, no need to do anything
                if let Some(placement_method) = placement_method {
                    let old = self
                        .placement_method_returns
                        .insert(placement_method.clone(), name.clone());
                    if let Some(old) = old {
                        if old.as_str() != name.as_str() {
                            bail!(
                                "Duplicate placement method `{}`, for both {} and {} (3)",
                                placement_method,
                                old,
                                name
                            )
                        }
                    }
                }
                Ok(())
            } else {
                // Already defined, but with different values... need to alias

                let mut alias_values = Vec::new();

                // Move previous definition
                let previous_code = self.code.remove(name).unwrap();
                let alias = Self::resolve_clash(name, defined)?;
                self.code.insert(alias.clone(), previous_code);
                self.aliases
                    .insert((name.clone(), defined.clone()), alias.clone());
                for (_, old_name) in &mut self.placement_method_returns {
                    if old_name.as_str() == name.as_str() {
                        old_name.clear();
                        old_name.push_str(alias.as_str());
                    }
                }
                alias_values.push((alias.clone(), defined.clone()));

                // Write new definition
                let alias = Self::resolve_clash(name, values)?;
                self.code.insert(alias.clone(), Self::codegen(values));
                self.aliases
                    .insert((name.clone(), values.clone()), alias.clone());
                if let Some(placement_method) = placement_method {
                    let old = self
                        .placement_method_returns
                        .insert(placement_method.clone(), alias.clone());
                    if let Some(old) = old {
                        if old.as_str() != alias.as_str() {
                            bail!(
                                "Duplicate placement method `{}`, for both {} and {} (4)",
                                placement_method,
                                old,
                                alias
                            )
                        }
                    }
                }
                alias_values.push((alias.clone(), values.clone()));

                // Insert already aliased
                self.already_aliased.insert(name.clone(), alias_values);

                Ok(())
            }
        } else {
            self.code.insert(String::from(name), Self::codegen(values));
            self.definitions.insert(name.clone(), values.clone());
            if let Some(placement_method) = placement_method {
                let old = self
                    .placement_method_returns
                    .insert(placement_method.clone(), name.clone());
                if let Some(old) = old {
                    if old.as_str() != name.as_str() {
                        bail!(
                            "Duplicate placement method `{}`, for both {} and {} (5)",
                            placement_method,
                            old,
                            name
                        )
                    }
                }
            }
            Ok(())
        }
    }

    fn resolve_clash(name: &str, values: &[String]) -> anyhow::Result<String> {
        let values: Vec<&'static str> = values
            .iter()
            .map(|f| unsafe { std::mem::transmute(f.as_str()) })
            .collect();

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
            code.push_str("#[repr(u8)]\n#[derive(Clone, Copy, Debug)]\npub enum ");
            code.push_str(&enum_name.to_case(Case::Pascal));
            code.push_str(enum_def);
        }
        code
    }

    fn get_placement_method_returns(&self) -> &IndexMap<String, String> {
        &self.placement_method_returns
    }

    fn get_aliases(&self) -> &IndexMap<(String, Vec<String>), String> {
        &self.aliases
    }

    fn get_parameter_name(&self, name: &String, values: &[String]) -> String {
        if let Some(name) = self.aliases.get(&(name.clone(), values.to_owned())) {
            name.clone()
        } else {
            name.to_case(Case::Pascal)
        }
    }
}
