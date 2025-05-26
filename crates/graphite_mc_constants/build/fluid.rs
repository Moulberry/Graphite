use std::collections::HashMap;
use std::{fmt::Write as _, collections::BTreeMap};
use std::io::Write;

use anyhow::bail;
use convert_case::{Case, Casing};
use indexmap::IndexMap;
use serde_derive::Deserialize;

use crate::property::{Property, PropertyWriter};
use crate::{file_src, file_out};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fluid {
    #[serde(default = "IndexMap::new")]
    properties: IndexMap<String, Property>,
    attributes: FluidAttributes,
    #[serde(default = "IndexMap::new")]
    state_attributes: IndexMap<String, FluidAttributes>,
    min_state_id: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FluidAttributes {
    #[serde(default)]
    own_height: Option<f32>,
    #[serde(default)]
    amount: Option<u8>,
    #[serde(default)]
    is_source: Option<bool>,
}

#[derive(PartialEq)]
struct ResolvedFluidAttributes {
    own_height: f32,
    amount: u8,
    is_source: bool,
}

pub fn write_fluid_states() -> anyhow::Result<()> {
    let raw_data = include_str!("../data/fluids.json");
    let mut fluids: IndexMap<String, Fluid> = serde_json::from_str(raw_data)?;
    fluids.sort_by(|_, value1, _, value2| value1.min_state_id.cmp(&value2.min_state_id));

    // Codegen all the parameters
    let mut parameter_writer: PropertyWriter = Default::default();
    for (_, fluid) in &fluids {
        for (name, parameter) in &fluid.properties {
            if let Property::String {
                values,
                default_value: _,
            } = parameter
            {
                parameter_writer.define_property(name, values, None)?;
            }
        }
    }

    let mut fluid_name_to_state_ids: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut fluid_name_to_id: BTreeMap<String, usize> = BTreeMap::new();

    let mut fluid_state_def = String::new();
    let mut fluid_def = String::new();
    let mut fluid_state_id_to_fluid_def = String::new();
    let mut u16_from_fluid_def = String::new();

    let mut state_lut: Vec<String> = Vec::new();
    let mut state_attributes_lut = String::new();
    let mut state_attribute_values_lut = String::new();
    let mut state_attribute_lookup: Vec<ResolvedFluidAttributes> = Vec::new();
    let mut shape_lut = String::new();
    let mut shape_lookup: Vec<Vec<[f64; 6]>> = Vec::new();

    let mut set_property_value_string = String::new();

    let mut state_count = 0;
    let mut fluid_id = 0;
    for (fluid_name, fluid) in &fluids {
        let min_state_id = fluid.min_state_id;
        let num_states = write_fluid_state(
            &mut fluid_state_def,
            &mut state_lut,
            &mut u16_from_fluid_def,
            &mut set_property_value_string,
            &parameter_writer,
            fluid_name,
            fluid,
            min_state_id,
        )?;

        fluid_def.push_str(&format!("\t{} = {},\n", &fluid_name.to_case(Case::Pascal), fluid_id));

        for _ in 0..num_states {
            fluid_state_id_to_fluid_def.push_str(&format!("\t\tFluid::{},\n", &fluid_name.to_case(Case::Pascal)));
        }

        state_count += num_states;
        let max_state_id = min_state_id + num_states;

        // Fluid Properties
        for state_id in min_state_id..max_state_id {
            if let Some(values) = fluid_name_to_state_ids.get_mut(&fluid_name.clone()) {
                values.push(state_id);
            } else {
                let mut values = Vec::new();
                values.push(state_id);
                fluid_name_to_state_ids.insert(fluid_name.clone(), values);
            }
            write_state_attributes(&mut state_attributes_lut, &mut state_attribute_values_lut, &mut shape_lut,
                &mut state_attribute_lookup, &mut shape_lookup, fluid_name, state_id, fluid)?;
        }

        fluid_name_to_id.insert(fluid_name.clone(), fluid_id);
        fluid_id += 1;
    }

    let mut write_buffer = String::new();

    // Write Main fluid.rs
    write_fluid_rs(&mut write_buffer, fluid_state_def, fluid_def)?;

    // Fluid Parameters
    let mut f = file_src("fluid_parameter.rs");
    f.write_all(parameter_writer.get_enum_code().as_bytes())?;

    // Fluid Tags
    // write_fluid_tags(fluid_name_to_state_ids, fluid_name_to_id, &mut write_buffer)?;

    // FluidState Into<u16>
    write_fluid_state_to_u16(&mut write_buffer, u16_from_fluid_def)?;

    // Fluid Into<u16>
    write_fluid_state_id_to_fluid(&mut write_buffer, state_count, fluid_state_id_to_fluid_def)?;

    // FluidState TryFrom<u16> + LUT
    write_u16_to_fluid(&mut write_buffer, state_count, state_lut)?;

    // Fluid Attributes
    write_u16_to_attributes(&mut write_buffer, state_count, state_attribute_lookup.len(),
        state_attributes_lut, state_attribute_values_lut, shape_lut)?;

    // Write set_property
    write_set_fluid_property(&mut write_buffer, set_property_value_string)?;

    Ok(())
}

fn write_fluid_rs(write_buffer: &mut String, fluid_state_def: String, fluid_def: String) -> Result<(), anyhow::Error> {
    write_buffer.push_str("use crate::fluid_parameter::*;\n\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/fluid_state_to_u16.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/u16_to_fluid_state.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/set_fluid_property.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/fluid_state_id_to_fluid.rs\"));\n");
    write_buffer
        .push_str("include!(concat!(env!(\"OUT_DIR\"), \"/fluid_attribute_lut.rs\"));\n\n");

    // Fluid State Id to Fluid
    write_buffer.push_str("pub fn state_to_fluid(id: u16) -> Result<Fluid, NoSuchFluidError> {\n");
    write_buffer
        .push_str("\tif id >= FLUID_STATE_ID_TO_FLUID.len() as _ { return Err(NoSuchFluidError(id)); }\n");
    write_buffer.push_str("\tOk(FLUID_STATE_ID_TO_FLUID[id as usize])\n");
    write_buffer.push_str("}\n\n");

    // Fluid Attributes
    write_buffer.push_str("#[derive(Debug)]\n");
    write_buffer.push_str("pub struct FluidAttributes {\n");
    write_buffer.push_str("\tpub own_height: f32,\n");
    write_buffer.push_str("\tpub amount: u8,\n");
    write_buffer.push_str("\tpub is_source: bool,\n");
    write_buffer.push_str("}\n");

    // Write u16 to FluidAttributes
    write_buffer.push_str(r#"
#[derive(Debug, thiserror::Error)]
#[error("No fluid exists for id: {0}")]
pub struct NoSuchFluidError(u16);

impl FluidAttributes {
    pub fn from_fluid_state(mut id: u16) -> &'static FluidAttributes {
        if id >= FLUID_ATTRIBUTE_INDEX_LUT.len() as _ {
            id = 0;
        }
        &FLUID_ATTRIBUTES_LUT[FLUID_ATTRIBUTE_INDEX_LUT[id as usize] as usize]
    }
}
"#);

    // Write FluidState Enum
    write_buffer.push_str("\n#[derive(Debug, Copy, Clone)]\npub enum FluidState {\n");
    write_buffer.push_str(&fluid_state_def);
    write_buffer.push_str("}\n\n");

    // Write Fluid Enum
    write_buffer.push_str("#[derive(Debug, Copy, Clone, PartialEq, Eq)]\n#[repr(u16)]\npub enum Fluid {\n");
    write_buffer.push_str(&fluid_def);
    write_buffer.push_str("}\n\n");

    let mut f = crate::file_src("fluid.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_u16_to_attributes(write_buffer: &mut String, state_count: usize, attribute_count: usize,
        state_attributes_lut: String, state_attribute_values_lut: String, shape_lut: String) -> Result<(), anyhow::Error> {
    writeln!(
        write_buffer,
        "const FLUID_ATTRIBUTE_INDEX_LUT: [u16; {}] = [",
        state_count
    )?;
    write_buffer.push_str(&state_attributes_lut);
    write_buffer.push_str("];\n\n");


    writeln!(
        write_buffer,
        "const FLUID_ATTRIBUTES_LUT: [FluidAttributes; {}] = [",
        attribute_count
    )?;
    write_buffer.push_str(&state_attribute_values_lut);
    write_buffer.push_str("];\n\n");

    write_buffer.push_str(&shape_lut);

    let mut f = crate::file_out("fluid_attribute_lut.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_u16_to_fluid(write_buffer: &mut String, state_count: usize, state_lut: Vec<String>) -> Result<(), anyhow::Error> {
    write_buffer.push_str("impl TryFrom<u16> for FluidState {\n");
    write_buffer.push_str("\ttype Error = NoSuchFluidError;");
    write_buffer.push_str("\tfn try_from(id: u16) -> Result<FluidState, Self::Error> {\n");
    write_buffer
        .push_str("\t\tif id >= FLUID_LUT.len() as _ { return Err(NoSuchFluidError(id)); }\n");
    write_buffer.push_str("\t\tOk(FLUID_LUT[id as usize])\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    writeln!(
        write_buffer,
        "const FLUID_LUT: [FluidState; {}] = [",
        state_count
    )?;
    for element in state_lut {
        write_buffer.push_str(&element);
        write_buffer.push_str(",\n");
    }
    write_buffer.push_str("];");

    let mut f = crate::file_out("u16_to_fluid_state.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_fluid_state_to_u16(write_buffer: &mut String, u16_from_fluid_def: String) -> Result<(), anyhow::Error> {
    write_buffer.push_str("impl From<&FluidState> for u16 {\n");
    write_buffer.push_str("\tfn from(fluid: &FluidState) -> u16 {\n");
    write_buffer.push_str("\t\tfluid.to_id()\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    write_buffer.push_str("impl FluidState {\n");
    write_buffer.push_str("\tpub const fn to_id(&self) -> u16 {\n");
    write_buffer.push_str("\t\tmatch self {\n");
    write_buffer.push_str(&u16_from_fluid_def);
    write_buffer.push_str("\t\t_ => 0\n");
    write_buffer.push_str("\t\t}\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    let mut f = crate::file_out("fluid_state_to_u16.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_fluid_state_id_to_fluid(write_buffer: &mut String, state_count: usize, fluid_state_id_to_fluid: String) -> Result<(), anyhow::Error> {
    writeln!(
        write_buffer,
        "const FLUID_STATE_ID_TO_FLUID: [Fluid; {}] = [",
        state_count
    )?;
    write_buffer.push_str(&fluid_state_id_to_fluid);
    write_buffer.push_str("];\n\n");

    let mut f = crate::file_out("fluid_state_id_to_fluid.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_string_to_u16(write_buffer: &mut String, string_to_u16_def: phf_codegen::Map<String>) -> Result<(), anyhow::Error> {
    write!(write_buffer, "static STRING_TO_U16: phf::Map<&'static str, u16> = {}", string_to_u16_def.build())?;
    write!(write_buffer, ";\n").unwrap();

    let mut f = crate::file_out("fluid_string_to_u16.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_set_fluid_property(write_buffer: &mut String, set_property_value_string: String) -> Result<(), anyhow::Error> {
    write_buffer.push_str("impl FluidState {\n");
    write_buffer.push_str("pub fn set_property(self, name: &str, value: &str) -> Option<FluidState> {\n");
    write_buffer.push_str("\tmatch self {\n");
    write_buffer.push_str(&set_property_value_string);
    write_buffer.push_str("\t\t_ => None\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    write_buffer.push_str("}\n");

    let mut f = crate::file_out("set_fluid_property.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_state_attributes(state_attributes_lut: &mut String, state_attribute_values_lut: &mut String,
        shape_lut: &mut String, state_attribute_lookup: &mut Vec<ResolvedFluidAttributes>, shape_lookup: &mut Vec<Vec<[f64; 6]>>,
        fluid_name: &String, state_id: usize, fluid: &Fluid) -> Result<(), anyhow::Error> {
    let mut own_height = fluid.attributes.own_height.unwrap_or(0.0);
    let mut amount = fluid.attributes.amount.unwrap_or(0);
    let mut is_source = fluid.attributes.is_source.unwrap_or(false);
    let state_attributes = fluid.state_attributes.get(&state_id.to_string());

    if let Some(state_attributes) = state_attributes {
        own_height = state_attributes.own_height.unwrap_or(own_height);
        amount = state_attributes.amount.unwrap_or(amount);
        is_source = state_attributes.is_source.unwrap_or(is_source);
    }

    let resolved = ResolvedFluidAttributes {
        own_height,
        amount,
        is_source
    };

    let index = if let Some(index) = state_attribute_lookup.iter().position(|r| r == &resolved) {
        index
    } else {
        writeln!(
            state_attribute_values_lut,
            "\tFluidAttributes {{ // eg. {} ({})",
            fluid_name, state_id
        )?;
    
        writeln!(state_attribute_values_lut, "\t\town_height: {}_f32,", own_height)?;
        writeln!(state_attribute_values_lut, "\t\tamount: {},", amount)?;
        writeln!(state_attribute_values_lut, "\t\tis_source: {},", is_source)?;
    
        state_attribute_values_lut.push_str("\t},\n");

        state_attribute_lookup.push(resolved);
        state_attribute_lookup.len() - 1
    };
    
    writeln!(
        state_attributes_lut,
        "\t{}, // {} ({})",
        index, fluid_name, state_id
    )?;

    Ok(())
}

fn write_fluid_state(
    fluid_def: &mut String,
    state_lut: &mut Vec<String>,
    u16_from_fluid_def: &mut String,
    set_property_value_string: &mut String,
    parameters: &PropertyWriter,
    fluid_name: &str,
    fluid: &Fluid,
    current_state_id: usize,
) -> anyhow::Result<usize> {
    let mut all_possible_parameters = Vec::new();

    fluid_def.push('\t');
    fluid_def.push_str(&fluid_name.to_case(Case::Pascal));

    let fluid_enum_ref = format!("FluidState::{}", fluid_name.to_case(Case::Pascal));

    if fluid.properties.is_empty() {
        fluid_def.push_str(" {},\n");

        writeln!(u16_from_fluid_def, "\t\t\t{} {{}} => {},", fluid_enum_ref, current_state_id)?;

        while state_lut.len() <= current_state_id {
            state_lut.push(String::new());
        }
        state_lut[current_state_id] = format!("\t{} {{}}", fluid_enum_ref);

        return Ok(1);
    } else {
        fluid_def.push_str(" {\n");

        // Emit eg. "FluidState::AcaciaButton{face, facing, powered} => {" for set_property method
        set_property_value_string.push_str("\t\tFluidState::");
        set_property_value_string.push_str(&fluid_name.to_case(Case::Pascal));
        set_property_value_string.push_str("{");
        let mut first = true;
        for key in fluid.properties.keys() {
            if first {
                first = false;
            } else {
                set_property_value_string.push_str(", ");
            }
            set_property_value_string.push_str(if key == "type" { "fluid_type" } else { key });
            if fluid.properties.len() == 1 {
                set_property_value_string.push_str(": _")
            }
        }
        set_property_value_string.push_str("}");

        set_property_value_string.push_str(" => {\n");
        set_property_value_string.push_str("\t\t\tmatch name {\n");

        for (name, state) in &fluid.properties {
            let field_name = if name == "type" { "fluid_type" } else { name };

            write!(set_property_value_string, "\t\t\t\t\"{name}\" => Some({fluid_enum_ref}{{")?;

            let mut first = true;
            for key in fluid.properties.keys() {
                if first {
                    first = false;
                } else {
                    set_property_value_string.push_str(", ");
                }
                set_property_value_string.push_str(if key == "type" { "fluid_type" } else { key });
                if key == name {
                    set_property_value_string.push_str(": value.parse().ok()?");
                }
            }
            set_property_value_string.push_str("}),\n");

            match state {
                Property::String {
                    values,
                    default_value: _,
                } => {
                    let parameter_name = parameters.get_parameter_name(name, values);

                    writeln!(fluid_def, "\t\t{field_name}: {parameter_name},")?;

                    let mut named_values = Vec::new();
                    for value in values {
                        let value = value.to_case(Case::Pascal);
                        named_values.push(format!("{field_name}: {parameter_name}::{},", value));
                    }
                    all_possible_parameters.push(named_values);
                }
                Property::Bool {
                    default_value: _,
                } => {
                    let mut named_values = Vec::new();
                    named_values.push(format!("{field_name}: true,"));
                    named_values.push(format!("{field_name}: false,"));
                    all_possible_parameters.push(named_values);

                    fluid_def.push_str("\t\t");
                    fluid_def.push_str(field_name);
                    fluid_def.push_str(": bool,\n");
                }
                Property::Int {
                    values,
                    default_value: _,
                } => {
                    fluid_def.push_str("\t\t");
                    fluid_def.push_str(field_name);
                    fluid_def.push_str(": u8,\n");

                    let mut named_values = Vec::new();
                    for value in values {
                        named_values.push(format!("{field_name}: {value},"));
                    }
                    all_possible_parameters.push(named_values);
                }
            }
        }
        fluid_def.push_str("\t},\n");
    }

    // Emit end of match for set_property method
    set_property_value_string.push_str("\t\t\t\t_ => None\n");
    set_property_value_string.push_str("\t\t\t}\n");
    set_property_value_string.push_str("\t\t},\n");

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
        state_def.push_str("\tFluidState::");
        state_def.push_str(&fluid_name.to_case(Case::Pascal));
        state_def.push('{');
        state_def.push_str(&one);
        state_def.push('}');

        // Push into LUT
        while state_lut.len() <= index {
            state_lut.push(String::new());
        }
        state_lut[index] = state_def.clone();

        // Push into From
        writeln!(u16_from_fluid_def, "\t\t{state_def} => {index},")?;

        index += 1;
    }

    Ok(all_count)
}