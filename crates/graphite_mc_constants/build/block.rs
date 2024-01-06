use std::collections::HashMap;
use std::{fmt::Write as _, collections::BTreeMap};
use std::io::Write;

use anyhow::bail;
use convert_case::{Case, Casing};
use indexmap::IndexMap;
use serde_derive::Deserialize;

use crate::{file_src, file_out};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    #[serde(default = "IndexMap::new")]
    pub properties: IndexMap<String, Property>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub custom_placer: Option<CustomPlacer>,
    // #[serde(default = "Vec::new")]
    // pub placement_conditions: Vec<String>,
    // #[serde(default)]
    // has_interaction: bool,
    #[serde(default)]
    corresponding_item: String,
    attributes: BlockAttributes,
    #[serde(default = "IndexMap::new")]
    state_attributes: IndexMap<String, BlockAttributes>,
    min_state_id: usize,
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
    is_west_face_sturdy: Option<bool>,
    #[serde(default)]
    is_up_face_sturdy: Option<bool>
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Property {
    #[serde(rename_all = "camelCase")]
    Int {
        values: Vec<i32>,
        default_value: i32,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // placement_value: Option<i32>,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // placement_method: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Bool {
        default_value: bool,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // placement_value: Option<bool>,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // placement_method: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    String {
        values: Vec<String>,
        default_value: String,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // placement_value: Option<String>,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // placement_method: Option<String>,
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
    HashMap<String, u16>
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
            } = parameter
            {
                parameter_writer.define_parameter(name, values, None)?;
            }
        }
    }

    let mut block_name_to_state_ids: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut block_name_to_id: BTreeMap<String, usize> = BTreeMap::new();

    let mut block_def = String::new();
    let mut u16_from_block_def = String::new();
    let mut item_lut = String::new();
    let mut state_lut: Vec<String> = Vec::new();
    let mut state_attributes_lut = String::new();
    let mut set_property_value_string = String::new();
    let mut perfect_string_to_u16 = phf_codegen::Map::new();
    let mut string_to_u16 = HashMap::new();

    let mut state_count = 0;
    let mut block_id = 0;
    for (block_name, block) in &blocks {
        let min_state_id = block.min_state_id;
        let num_states = write_block_state(
            &mut block_def,
            &mut state_lut,
            &mut u16_from_block_def,
            &mut set_property_value_string,
            &parameter_writer,
            block_name,
            block,
            min_state_id,
        )?;

        state_count += num_states;
        let max_state_id = min_state_id + num_states;

        perfect_string_to_u16.entry(format!("minecraft:{}", block_name), &format!("{}_u16", min_state_id));
        string_to_u16.insert(format!("minecraft:{}", block_name), min_state_id as u16);

        // Block Properties
        let mut item_pascal = block.corresponding_item.replace("minecraft:", "").to_case(Case::Pascal);
        if item_pascal.is_empty() {
            item_pascal = "Air".into();
        }
        for state_id in min_state_id..max_state_id {
            writeln!(item_lut, "\t\tcrate::item::Item::{}, // Block: {}", item_pascal, block_name)?;

            if let Some(values) = block_name_to_state_ids.get_mut(&block_name.clone()) {
                values.push(state_id);
            } else {
                let mut values = Vec::new();
                values.push(state_id);
                block_name_to_state_ids.insert(block_name.clone(), values);
            }
            write_state_attributes(&mut state_attributes_lut, block_name, state_id, block)?;
        }

        block_name_to_id.insert(block_name.clone(), block_id);
        block_id += 1;
    }

    let mut write_buffer = String::new();

    // Write Main block.rs
    write_block_rs(&mut write_buffer, block_def)?;

    // Block Parameters
    let mut f = file_src("block_parameter.rs");
    f.write_all(parameter_writer.get_enum_code().as_bytes())?;

    // Block Tags
    // write_block_tags(block_name_to_state_ids, block_name_to_id, &mut write_buffer)?;

    // Block Into<u16>
    write_block_to_u16(&mut write_buffer, u16_from_block_def)?;

    // Block TryFrom<u16> + LUT
    write_u16_to_block(&mut write_buffer, state_count, state_lut)?;

    // Item from u16 + LUT
    write_state_to_item(&mut write_buffer, state_count, item_lut)?;

    // Block Attributes
    write_u16_to_attributes(&mut write_buffer, state_count, state_attributes_lut)?;

    // String to u16
    write_string_to_u16(&mut write_buffer, perfect_string_to_u16)?;

    // String to u16
    write_set_block_property(&mut write_buffer, set_property_value_string)?;

    Ok((
        parameter_writer.get_placement_method_returns().clone(),
        parameter_writer.get_aliases().clone(),
        string_to_u16
    ))
}

fn write_state_to_item(write_buffer: &mut String, state_count: usize, item_lut: String) -> Result<(), anyhow::Error> {
    writeln!(
        write_buffer,
        "const ITEM_LUT: [crate::item::Item; {}] = [",
        state_count
    )?;
    write_buffer.push_str(&item_lut);
    write_buffer.push_str("];");

    let mut f = file_out("u16_to_item.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();

    Ok(())
}

// fn write_block_tags(block_name_to_state_ids: BTreeMap<String, Vec<usize>>, block_name_to_id: BTreeMap<String, usize>, write_buffer: &mut String) -> Result<(), anyhow::Error> {
//     let tags_data = include_str!("../data/tags/block_tags.json");
//     let block_tags: IndexMap<String, IndexMap<String, Vec<String>>> = serde_json::from_str(tags_data)?;
//     let mut tag_name_to_states: IndexMap<String, (BTreeSet<usize>, BTreeSet<usize>, Vec<String>)> = IndexMap::new();
//     for (tag_name, value_map) in block_tags {
//         let tag_name = tag_name.replace("minecraft:", "");
//         if let Some(values) = value_map.get("values") {
//             let mut state_ids_set: BTreeSet<usize> = BTreeSet::new();
//             let mut ids_set: BTreeSet<usize> = BTreeSet::new();
//             let mut references = Vec::new();
//             for value in values {
//                 let value = value.replace("minecraft:", "");
//                 if value.starts_with("#") {
//                     let tag_reference = value.replace("#", "");
//                     references.push(tag_reference);
//                     continue;
//                 }
                
//                 if let Some(states) = block_name_to_state_ids.get(&value) {
//                     state_ids_set.extend(states);
//                 } else {
//                     panic!("unable to find {} in block_name_to_state_ids", value);
//                 }

//                 if let Some(id) = block_name_to_id.get(&value) {
//                     ids_set.insert(*id);
//                 } else {
//                     panic!("unable to find {} in block_name_to_id", value);
//                 }
//             }
//             tag_name_to_states.insert(tag_name.clone(), (state_ids_set, ids_set, references));
//         }
//     }
//     let mut has_some_references = true;
//     while has_some_references {
//         has_some_references = false;

//         let cloned = tag_name_to_states.clone();

//         for (_, (state_ids, ids, references)) in &mut tag_name_to_states {
//             if !references.is_empty() {
//                 let mut new_references = Vec::new();

//                 has_some_references = true;

//                 for reference in references.iter() {
//                     if let Some((ref_state_ids, ref_ids, ref_references)) = cloned.get(reference) {
//                         new_references.extend(ref_references.clone());
//                         state_ids.extend(ref_state_ids);
//                         ids.extend(ref_ids);
//                     }
//                 }

//                 *references = new_references;
//             }
//         }
//     }
//     write_buffer.push_str("#[derive(Debug, Clone, Copy)]\n");
//     write_buffer.push_str("pub enum BlockTags {\n");
//     for (tag_name, _) in &tag_name_to_states {
//         let tag_name_pascal = tag_name.replace("/", "_").to_case(Case::Pascal);
//         writeln!(write_buffer, "\t{},", tag_name_pascal)?;
//     }
//     write_buffer.push_str("}\n\n");

//     write_buffer.push_str("impl BlockTags {\n");
//     write_buffer.push_str("\tpub fn to_namespace(self) -> &'static str {\n");
//     write_buffer.push_str("\t\tmatch self {\n");
//     for (tag_name, _) in &tag_name_to_states {
//         let tag_name_pascal = tag_name.replace("/", "_").to_case(Case::Pascal);
//         writeln!(write_buffer, "\t\t\tSelf::{} => \"{}\",", tag_name_pascal, tag_name)?;
//     }
//     write_buffer.push_str("\t\t}\n");
//     write_buffer.push_str("\t}\n\n");

//     write_buffer.push_str("\tpub fn iter() -> &'static [Self] {\n");
//     write_buffer.push_str("\t\t&[\n");
//     for (tag_name, _) in &tag_name_to_states {
//         let tag_name_pascal = tag_name.replace("/", "_").to_case(Case::Pascal);
//         writeln!(write_buffer, "\t\t\tSelf::{},", tag_name_pascal)?;
//     }
//     write_buffer.push_str("\t\t]\n");
//     write_buffer.push_str("\t}\n");

//     write_buffer.push_str("\tpub fn values(self) -> &'static [u16] {\n");
//     write_buffer.push_str("\t\tmatch self {\n");
//     for (tag_name, _) in &tag_name_to_states {
//         let tag_name_pascal = tag_name.replace("/", "_").to_case(Case::Pascal);
//         let tag_name_ss = tag_name.replace("/", "_").to_case(Case::ScreamingSnake);
//         writeln!(write_buffer, "\t\t\tSelf::{} => &{},", tag_name_pascal, tag_name_ss)?;
//     }
//     write_buffer.push_str("\t\t}\n");
//     write_buffer.push_str("\t}\n\n");

//     write_buffer.push_str("\tpub fn contains(self, state: u16) -> bool {\n");
//     write_buffer.push_str("\t\tmatch self {\n");
//     for (tag_name, _) in &tag_name_to_states {
//         let tag_name_pascal = tag_name.replace("/", "_").to_case(Case::Pascal);
//         let tag_name_ss = tag_name.replace("/", "_").to_case(Case::ScreamingSnake);
//         writeln!(write_buffer, "\t\t\tSelf::{} => {}_STATES.binary_search(&state).is_ok(),", tag_name_pascal, tag_name_ss)?;
//     }
//     write_buffer.push_str("\t\t}\n");
//     write_buffer.push_str("\t}\n");
//     write_buffer.push_str("}\n\n");

//     for (tag_name, (state_ids, ids, _)) in tag_name_to_states {
//         let tag_name_ss = tag_name.replace("/", "_").to_case(Case::ScreamingSnake);

//         write!(write_buffer, "pub const {}_STATES: [u16; {}] = [\n\t", tag_name_ss, state_ids.len())?;
//         for value in state_ids {
//             write!(write_buffer, "{}, ", value)?;
//         }
//         write_buffer.push_str("\n];\n");

//         write!(write_buffer, "pub const {}: [u16; {}] = [\n\t", tag_name_ss, ids.len())?;
//         for value in ids {
//             write!(write_buffer, "{}, ", value)?;
//         }
//         write_buffer.push_str("\n];\n");
//     }

//     let mut f = file_src("tags/block.rs");
//     f.write_all(write_buffer.as_bytes())?;
//     write_buffer.clear();
//     Ok(())
// }

fn write_block_rs(write_buffer: &mut String, block_def: String) -> Result<(), anyhow::Error> {
    write_buffer.push_str("use crate::block_parameter::*;\n\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/block_to_u16.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/string_to_u16.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/u16_to_block.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/u16_to_item.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/set_block_property.rs\"));\n");
    write_buffer
        .push_str("include!(concat!(env!(\"OUT_DIR\"), \"/u16_to_block_attributes.rs\"));\n\n");

    // Item Lookup
    write_buffer.push_str("pub fn state_to_item(id: u16) -> Result<crate::item::Item, NoSuchBlockError> {\n");
    write_buffer
        .push_str("\tif id >= ITEM_LUT.len() as _ { return Err(NoSuchBlockError(id)); }\n");
    write_buffer.push_str("\tOk(ITEM_LUT[id as usize])\n");
    write_buffer.push_str("}\n\n");


    // String->Block Lookup
    write_buffer.push_str("pub fn string_to_u16(string: &str) -> Option<u16> {\n");
    write_buffer.push_str("\tSTRING_TO_U16.get(string).copied()\n");
    write_buffer.push_str("}\n\n");

    // Block Attributes
    write_buffer.push_str("#[derive(Debug)]\n");
    write_buffer.push_str("pub struct BlockAttributes {\n");
    write_buffer.push_str("\tpub hardness: f32,\n");
    write_buffer.push_str("\tpub replaceable: bool,\n");
    write_buffer.push_str("\tpub air: bool,\n");
    write_buffer.push_str("\tpub is_north_face_sturdy: bool,\n");
    write_buffer.push_str("\tpub is_east_face_sturdy: bool,\n");
    write_buffer.push_str("\tpub is_south_face_sturdy: bool,\n");
    write_buffer.push_str("\tpub is_west_face_sturdy: bool,\n");
    write_buffer.push_str("\tpub is_up_face_sturdy: bool,\n");
    write_buffer.push_str("}\n\n");

    // Write Error
    write_buffer.push_str("#[derive(Debug, thiserror::Error)]\n");
    write_buffer.push_str("#[error(\"No block exists for id: {0}\")]\n");
    write_buffer.push_str("pub struct NoSuchBlockError(u16);\n\n");

    // State Id to Item


    // Write Block Enum
    write_buffer.push_str("#[derive(Debug, Copy, Clone)]\npub enum Block {\n");
    write_buffer.push_str(&block_def);
    write_buffer.push_str("}\n\n");

    let mut f = crate::file_src("block.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_u16_to_attributes(write_buffer: &mut String, state_count: usize, state_attributes_lut: String) -> Result<(), anyhow::Error> {
    write_buffer.push_str("impl TryFrom<u16> for &BlockAttributes {\n");
    write_buffer.push_str("\ttype Error = NoSuchBlockError;");
    write_buffer
        .push_str("\tfn try_from(id: u16) -> Result<&'static BlockAttributes, Self::Error> {\n");
    write_buffer.push_str(
        "\t\tif id >= BLOCK_ATTRIBUTES_LUT.len() as _ { return Err(NoSuchBlockError(id)); }\n",
    );
    write_buffer.push_str("\t\tOk(&BLOCK_ATTRIBUTES_LUT[id as usize])\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    writeln!(
        write_buffer,
        "const BLOCK_ATTRIBUTES_LUT: [BlockAttributes; {}] = [",
        state_count
    )?;
    write_buffer.push_str(&state_attributes_lut);
    write_buffer.push_str("];");
    let mut f = crate::file_out("u16_to_block_attributes.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_u16_to_block(write_buffer: &mut String, state_count: usize, state_lut: Vec<String>) -> Result<(), anyhow::Error> {
    write_buffer.push_str("impl TryFrom<u16> for Block {\n");
    write_buffer.push_str("\ttype Error = NoSuchBlockError;");
    write_buffer.push_str("\tfn try_from(id: u16) -> Result<Block, Self::Error> {\n");
    write_buffer
        .push_str("\t\tif id >= BLOCK_LUT.len() as _ { return Err(NoSuchBlockError(id)); }\n");
    write_buffer.push_str("\t\tOk(BLOCK_LUT[id as usize])\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    writeln!(
        write_buffer,
        "const BLOCK_LUT: [Block; {}] = [",
        state_count
    )?;
    for element in state_lut {
        write_buffer.push_str(&element);
        write_buffer.push_str(",\n");
    }
    write_buffer.push_str("];");

    let mut f = crate::file_out("u16_to_block.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_block_to_u16(write_buffer: &mut String, u16_from_block_def: String) -> Result<(), anyhow::Error> {
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
    Ok(())
}

fn write_string_to_u16(write_buffer: &mut String, string_to_u16_def: phf_codegen::Map<String>) -> Result<(), anyhow::Error> {
    write!(write_buffer, "static STRING_TO_U16: phf::Map<&'static str, u16> = {}", string_to_u16_def.build())?;
    write!(write_buffer, ";\n").unwrap();

    let mut f = crate::file_out("string_to_u16.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_set_block_property(write_buffer: &mut String, set_property_value_string: String) -> Result<(), anyhow::Error> {
    write_buffer.push_str("impl Block {\n");
    write_buffer.push_str("pub fn set_property(self, name: &str, value: &str) -> Option<Block> {\n");
    write_buffer.push_str("\tmatch self {\n");
    write_buffer.push_str(&set_property_value_string);
    write_buffer.push_str("\t\t_ => None\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    write_buffer.push_str("}\n");

    let mut f = crate::file_out("set_block_property.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())


    /*
    pub fn set_property(block: Block, name: &str, value: &str) -> Option<Block> {
	match block {
		Block::AcaciaButton{face, facing, powered} => {
			match name {
				"face" => Some(Block::AcaciaButton{ face: value.parse().ok()?, facing, powered }),
				"facing" => Some(Block::AcaciaButton{ face, facing: value.parse().ok()?, powered }),
				"powered" => Some(Block::AcaciaButton{ face, facing, powered: value.parse().ok()? }),
				_ => None
			}
		},
		_ => None
	}
    }
    */
}

fn write_state_attributes(state_attributes_lut: &mut String, block_name: &String, state_id: usize, block: &Block) -> Result<(), anyhow::Error> {
    writeln!(
        state_attributes_lut,
        "\tBlockAttributes {{ // {} ({})",
        block_name, state_id
    )?;

    let mut hardness = block.attributes.hardness.unwrap_or(0.0);
    let mut replaceable = block.attributes.replaceable.unwrap_or(false);
    let mut air = block.attributes.air.unwrap_or(false);
    let mut is_north_face_sturdy = block.attributes.is_north_face_sturdy.unwrap_or(true);
    let mut is_east_face_sturdy = block.attributes.is_east_face_sturdy.unwrap_or(true);
    let mut is_south_face_sturdy = block.attributes.is_south_face_sturdy.unwrap_or(true);
    let mut is_west_face_sturdy = block.attributes.is_west_face_sturdy.unwrap_or(true);
    let mut is_up_face_sturdy = block.attributes.is_up_face_sturdy.unwrap_or(true);
    let state_attributes = block.state_attributes.get(&state_id.to_string());

    if let Some(state_attributes) = state_attributes {
        hardness = state_attributes.hardness.unwrap_or(hardness);
        replaceable = state_attributes.replaceable.unwrap_or(replaceable);
        air = state_attributes.air.unwrap_or(air);
        is_north_face_sturdy = state_attributes.is_north_face_sturdy.unwrap_or(is_north_face_sturdy);
        is_east_face_sturdy = state_attributes.is_east_face_sturdy.unwrap_or(is_east_face_sturdy);
        is_south_face_sturdy = state_attributes.is_south_face_sturdy.unwrap_or(is_south_face_sturdy);
        is_west_face_sturdy = state_attributes.is_west_face_sturdy.unwrap_or(is_west_face_sturdy);
        is_up_face_sturdy = state_attributes.is_up_face_sturdy.unwrap_or(is_up_face_sturdy);
    }

    writeln!(state_attributes_lut, "\t\thardness: {}_f32,", hardness)?;
    writeln!(state_attributes_lut, "\t\treplaceable: {},", replaceable)?;
    writeln!(state_attributes_lut, "\t\tair: {},", air)?;
    writeln!(state_attributes_lut, "\t\tis_north_face_sturdy: {},", is_north_face_sturdy)?;
    writeln!(state_attributes_lut, "\t\tis_east_face_sturdy: {},", is_east_face_sturdy)?;
    writeln!(state_attributes_lut, "\t\tis_south_face_sturdy: {},", is_south_face_sturdy)?;
    writeln!(state_attributes_lut, "\t\tis_west_face_sturdy: {},", is_west_face_sturdy)?;
    writeln!(state_attributes_lut, "\t\tis_up_face_sturdy: {},", is_up_face_sturdy)?;

    state_attributes_lut.push_str("\t},\n");
    Ok(())
}

fn write_block_state(
    block_def: &mut String,
    state_lut: &mut Vec<String>,
    u16_from_block_def: &mut String,
    set_property_value_string: &mut String,
    parameters: &ParameterWriter,
    block_name: &str,
    block: &Block,
    current_state_id: usize,
) -> anyhow::Result<usize> {
    let mut all_possible_parameters = Vec::new();

    block_def.push('\t');
    block_def.push_str(&block_name.to_case(Case::Pascal));

    let block_enum_ref = format!("Block::{}", block_name.to_case(Case::Pascal));

    if block.properties.is_empty() {
        block_def.push_str(",\n");

        writeln!(u16_from_block_def, "\t\t\t{} => {},", block_enum_ref, current_state_id)?;

        while state_lut.len() <= current_state_id {
            state_lut.push(String::new());
        }
        state_lut[current_state_id] = format!("\t{}", block_enum_ref);

        return Ok(1);
    } else {
        block_def.push_str(" {\n");

        // Emit eg. "Block::AcaciaButton{face, facing, powered} => {" for set_property method
        set_property_value_string.push_str("\t\tBlock::");
        set_property_value_string.push_str(&block_name.to_case(Case::Pascal));
        set_property_value_string.push_str("{");
        let mut first = true;
        for key in block.properties.keys() {
            if first {
                first = false;
            } else {
                set_property_value_string.push_str(", ");
            }
            set_property_value_string.push_str(if key == "type" { "block_type" } else { key });
            if block.properties.len() == 1 {
                set_property_value_string.push_str(": _")
            }
        }
        set_property_value_string.push_str("}");

        set_property_value_string.push_str(" => {\n");
        set_property_value_string.push_str("\t\t\tmatch name {\n");

        for (name, state) in &block.properties {
            let field_name = if name == "type" { "block_type" } else { name };

            write!(set_property_value_string, "\t\t\t\t\"{name}\" => Some({block_enum_ref}{{")?;

            let mut first = true;
            for key in block.properties.keys() {
                if first {
                    first = false;
                } else {
                    set_property_value_string.push_str(", ");
                }
                set_property_value_string.push_str(if key == "type" { "block_type" } else { key });
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

                    writeln!(block_def, "\t\t{field_name}: {parameter_name},")?;

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

                    block_def.push_str("\t\t");
                    block_def.push_str(field_name);
                    block_def.push_str(": bool,\n");
                }
                Property::Int {
                    values,
                    default_value: _,
                } => {
                    block_def.push_str("\t\t");
                    block_def.push_str(field_name);
                    block_def.push_str(": u8,\n");

                    let mut named_values = Vec::new();
                    for value in values {
                        named_values.push(format!("{field_name}: {value},"));
                    }
                    all_possible_parameters.push(named_values);
                }
            }
        }
        block_def.push_str("\t},\n");
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
        state_def.push_str("\tBlock::");
        state_def.push_str(&block_name.to_case(Case::Pascal));
        state_def.push('{');
        state_def.push_str(&one);
        state_def.push('}');

        // Push into LUT
        while state_lut.len() <= index {
            state_lut.push(String::new());
        }
        state_lut[index] = state_def.clone();

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
        let values: Vec<&str> = values
            .iter()
            .map(|f| f.as_str())
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
            code.push_str("#[repr(u8)]\n#[derive(Clone, Copy, Debug, PartialEq, Eq, strum_macros::EnumString)]\n#[strum(serialize_all = \"snake_case\")]\npub enum ");
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
