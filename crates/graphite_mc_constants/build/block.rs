use std::collections::{HashMap, HashSet};
use std::ops::RangeInclusive;
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
pub struct Block {
    #[serde(default = "IndexMap::new")]
    pub properties: IndexMap<String, Property>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub custom_placer: Option<CustomPlacer>,
    // #[serde(default = "Vec::new")]
    // pub placement_conditions: Vec<String>,
    // #[serde(default)]
    // has_interaction: bool,
    class: String,
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
    fluid_state: Option<u16>,
    #[serde(default)]
    hardness: Option<f32>,
    #[serde(default)]
    friction: Option<f32>,
    #[serde(default)]
    speed_factor: Option<f32>,
    #[serde(default)]
    jump_factor: Option<f32>,
    #[serde(default)]
    replaceable: Option<bool>,
    #[serde(default)]
    air: Option<bool>,
    #[serde(default)]
    blocks_motion: Option<bool>,
    #[serde(default)]
    waterlogged: Option<bool>,
    #[serde(default)]
    suffocating: Option<bool>,
    #[serde(default)]
    is_sturdy_north: Option<bool>,
    #[serde(default)]
    is_sturdy_east: Option<bool>,
    #[serde(default)]
    is_sturdy_south: Option<bool>,
    #[serde(default)]
    is_sturdy_west: Option<bool>,
    #[serde(default)]
    is_pathfindable_land: Option<bool>,
    #[serde(default)]
    is_pathfindable_air: Option<bool>,
    #[serde(default)]
    is_pathfindable_water: Option<bool>,
    #[serde(default)]
    light_emission: Option<u8>,
    #[serde(default)]
    fall_damage_resetting: Option<bool>,
    #[serde(default)]
    climbable: Option<bool>,
    #[serde(default)]
    clockwise_rotation_state_id_offset: Option<i16>,
    #[serde(default)]
    collision_shape: Vec<[f64; 6]>
}

#[derive(PartialEq)]
struct ResolvedBlockAttributes {
    fluid_state: u16,
    hardness: f32,
    friction: f32,
    speed_factor: f32,
    jump_factor: f32,
    replaceable: bool,
    air: bool,
    blocks_motion: bool,
    waterlogged: bool,
    suffocating: bool,
    is_sturdy_north: bool,
    is_sturdy_east: bool,
    is_sturdy_south: bool,
    is_sturdy_west: bool,
    is_pathfindable_land: bool,
    is_pathfindable_air: bool,
    is_pathfindable_water: bool,
    light_emission: u8,
    fall_damage_resetting: bool,
    climbable: bool,
    clockwise_rotation_state_id_offset: i16,
    collision_bounds: Option<usize>,
    collision_shape: usize
}

pub fn write_block_states() -> anyhow::Result<(
    IndexMap<String, String>,
    IndexMap<(String, Vec<String>), String>,
    HashMap<String, u16>
)> {
    let raw_data = include_str!("../data/blocks.json");
    let mut blocks: IndexMap<String, Block> = serde_json::from_str(raw_data)?;
    blocks.sort_by(|_, value1, _, value2| value1.min_state_id.cmp(&value2.min_state_id));

    // Codegen all the parameters
    let mut parameter_writer: PropertyWriter = Default::default();
    for (_, block) in &blocks {
        for (name, parameter) in &block.properties {
            if let Property::String {
                values,
                default_value: _,
            } = parameter
            {
                parameter_writer.define_property(name, values, None)?;
            }
        }
    }

    let mut block_name_to_state_ids: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut block_name_to_id: BTreeMap<String, usize> = BTreeMap::new();

    let mut block_state_def = String::new();
    let mut block_class_def = String::new();
    let mut used_classes = HashSet::new();
    let mut block_def = String::new();
    let mut block_state_id_to_block_def = String::new();
    let mut u16_from_block_def = String::new();
    let mut item_lut = String::new();

    let mut state_lut: Vec<String> = Vec::new();
    let mut class_lut: Vec<String> = Vec::new();
    let mut state_attributes_lut = String::new();
    let mut state_attribute_values_lut = String::new();
    let mut state_attribute_lookup: Vec<ResolvedBlockAttributes> = Vec::new();
    let mut shapes = String::new();
    let mut shape_lookup: Vec<Vec<[f64; 6]>> = Vec::new();
    let mut bounds_lookup: Vec<[f64; 6]> = Vec::new();

    let mut set_property_value_string = String::new();
    let mut perfect_string_to_u16 = phf_codegen::Map::new();
    let mut string_to_u16 = HashMap::new();
    let mut block_to_state_range = Vec::new();

    let mut state_count = 0;
    let mut block_id = 0;
    for (block_name, block) in &blocks {
        let min_state_id = block.min_state_id;
        let num_states = write_block_state(
            &mut block_state_def,
            &mut block_class_def,
            &mut used_classes,
            &mut state_lut,
            &mut class_lut,
            &mut u16_from_block_def,
            &mut set_property_value_string,
            &parameter_writer,
            block_name,
            block,
            min_state_id,
        )?;

        block_def.push_str(&format!("\t{} = {},\n", &block_name.to_case(Case::Pascal), block_id));

        for _ in 0..num_states {
            block_state_id_to_block_def.push_str(&format!("\t\tBlock::{},\n", &block_name.to_case(Case::Pascal)));
        }

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
            write_state_attributes(&mut state_attributes_lut, &mut state_attribute_values_lut, &mut shapes,
                &mut state_attribute_lookup, &mut shape_lookup, &mut bounds_lookup, block_name, state_id, block)?;
        }
        block_to_state_range.push(format!("\t\t\tBlock::{} => {} ..= {},\n", &block_name.to_case(Case::Pascal), min_state_id, max_state_id-1));

        block_name_to_id.insert(block_name.clone(), block_id);
        block_id += 1;
    }

    let mut write_buffer = String::new();

    // Write Main block.rs
    write_block_rs(&mut write_buffer, block_state_def, block_class_def, block_def, block_to_state_range)?;

    // Block Parameters
    let mut f = file_src("block_parameter.rs");
    f.write_all(parameter_writer.get_enum_code().as_bytes())?;

    // Block Tags
    // write_block_tags(block_name_to_state_ids, block_name_to_id, &mut write_buffer)?;

    // BlockState Into<u16>
    write_block_state_to_u16(&mut write_buffer, u16_from_block_def)?;

    // Block Into<u16>
    write_block_state_id_to_block(&mut write_buffer, state_count, block_state_id_to_block_def)?;

    // BlockState TryFrom<u16> + LUT
    write_u16_to_block_state(&mut write_buffer, state_count, state_lut)?;

    // BlockClass TryFrom<u16> + LUT
    write_u16_to_block_class(&mut write_buffer, state_count, class_lut)?;

    // Item from u16 + LUT
    write_state_to_item(&mut write_buffer, state_count, item_lut)?;

    // Block Attributes
    write_attribute_lut(&mut write_buffer, state_count, state_attribute_lookup.len(),
        state_attributes_lut, state_attribute_values_lut, shapes, shape_lookup.len())?;

    // String to u16
    write_string_to_u16(&mut write_buffer, perfect_string_to_u16)?;

    // Write set_property
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

fn write_block_rs(write_buffer: &mut String, block_state_def: String, block_class_def: String, block_def: String, block_to_state_range: Vec<String>) -> Result<(), anyhow::Error> {
    write_buffer.push_str("use crate::block_parameter::*;\n\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/block_state_to_u16.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/block_string_to_u16.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/u16_to_block_state.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/u16_to_block_class.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/u16_to_item.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/set_block_property.rs\"));\n");
    write_buffer.push_str("include!(concat!(env!(\"OUT_DIR\"), \"/block_state_id_to_block.rs\"));\n");
    write_buffer
        .push_str("include!(concat!(env!(\"OUT_DIR\"), \"/block_attribute_lut.rs\"));\n\n");

    // Item Lookup
    write_buffer.push_str("pub fn state_to_item(id: u16) -> Result<crate::item::Item, NoSuchBlockError> {\n");
    write_buffer
        .push_str("\tif id >= ITEM_LUT.len() as _ { return Err(NoSuchBlockError(id)); }\n");
    write_buffer.push_str("\tOk(ITEM_LUT[id as usize])\n");
    write_buffer.push_str("}\n\n");

    // Block State Id to Block
    write_buffer.push_str("pub fn state_to_block(id: u16) -> Result<Block, NoSuchBlockError> {\n");
    write_buffer
        .push_str("\tif id >= BLOCK_STATE_ID_TO_BLOCK.len() as _ { return Err(NoSuchBlockError(id)); }\n");
    write_buffer.push_str("\tOk(BLOCK_STATE_ID_TO_BLOCK[id as usize])\n");
    write_buffer.push_str("}\n\n");

    // String->Block Lookup
    write_buffer.push_str("pub fn string_to_u16(string: &str) -> Option<u16> {\n");
    write_buffer.push_str("\tSTRING_TO_U16.get(string).copied()\n");
    write_buffer.push_str("}\n\n");

    // Helper for parsing block
    write_buffer.push_str(r#"
pub fn parse_block_state(block: graphite_binary::nbt::CompoundRef<'_>) -> u16 {
    let Some(name) = block.find_string("Name") else {
        return 0;
    };

    let mut name = name.as_ref();
	if name == "minecraft:grass" {
		name = "minecraft:short_grass";
	}
    
    let Some(mut id) = string_to_u16(name) else {
        return 0;
    };

    if let Some(properties) = block.find_compound("Properties") {
        if !properties.is_empty() {
            let mut block: BlockState = id.try_into().unwrap();
            for (key, value) in properties.entries() {
                if let Some(value_str) = value.as_string() {
                    block = block.set_property(key, value_str).unwrap_or(block);
                }
            }
            id = block.to_id();
        }
    }

    id
}"#);

    write_buffer.push_str("\n");

    write_buffer.push_str(
r#"
#[derive(Debug, enumset::EnumSetType)]
pub enum BlockFlag {
    Replaceable,
    Air,
    BlocksMotion,
    Waterlogged,
    Suffocating,
    IsSturdyNorth,
    IsSturdyEast,
    IsSturdySouth,
    IsSturdyWest,
    IsPathfindableLand,
    IsPathfindableAir,
    IsPathfindableWater,
    FallDamageResetting,
    Climbable,
}

#[derive(Debug)]
pub struct BlockAttributes {
    pub flags: enumset::EnumSet<BlockFlag>,
    pub fluid_state: u16,
    pub hardness: f32,
    pub friction: f32,
    pub speed_factor: f32,
    pub jump_factor: f32,
    pub light_emission: u8,
    pub clockwise_rotation_state_id_offset: i16,
    pub collision_bounds: Option<&'static [f64; 6]>,
    pub collision_shape: &'static [[f64; 6]],
    pub collision_shape_id: u16,
}

impl BlockAttributes {
    #[inline(always)]
    pub fn has_flag(&self, flag: BlockFlag) -> bool {
        self.flags.contains(flag)
    }
}
"#);

    // Write u16 to BlockAttributes
    write_buffer.push_str(r#"
#[derive(Debug, thiserror::Error)]
#[error("No block exists for id: {0}")]
pub struct NoSuchBlockError(u16);

impl BlockAttributes {
    pub fn from_block_state(mut id: u16) -> &'static BlockAttributes {
        if id >= BLOCK_ATTRIBUTE_INDEX_LUT.len() as _ {
            id = 0;
        }
        &BLOCK_ATTRIBUTES_LUT[BLOCK_ATTRIBUTE_INDEX_LUT[id as usize] as usize]
    }
}
"#);

    // Write BlockState Enum
    write_buffer.push_str("\n#[derive(Debug, Copy, Clone)]\npub enum BlockState {\n");
    write_buffer.push_str(&block_state_def);
    write_buffer.push_str("}\n\n");

    // Write BlockClass Enum
    write_buffer.push_str("\n#[derive(Debug, Copy, Clone)]\npub enum BlockClass {\n");
    write_buffer.push_str(&block_class_def);
    write_buffer.push_str("}\n\n");

    // Write Block Enum
    write_buffer.push_str("#[derive(Debug, Copy, Clone, PartialEq, Eq)]\n#[repr(u16)]\npub enum Block {\n");
    write_buffer.push_str(&block_def);
    write_buffer.push_str("}\n\n");

    write_buffer.push_str(r#"
impl Block {
    pub const fn block_state_range(self) -> std::ops::RangeInclusive<u16> {
        match self {  
"#);

    for string in block_to_state_range {
        write_buffer.push_str(&string);
    }

    write_buffer.push_str(r#"    }
    }
}"#);


    let mut f = crate::file_src("block.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_attribute_lut(write_buffer: &mut String, state_count: usize, attribute_count: usize,
        state_attributes_lut: String, state_attribute_values_lut: String, shapes: String, shape_count: usize) -> Result<(), anyhow::Error> {
    writeln!(
        write_buffer,
        "const BLOCK_ATTRIBUTE_INDEX_LUT: [u16; {}] = [",
        state_count
    )?;
    write_buffer.push_str(&state_attributes_lut);
    write_buffer.push_str("];\n\n");

    writeln!(
        write_buffer,
        "const BLOCK_ATTRIBUTES_LUT: [BlockAttributes; {}] = [",
        attribute_count
    )?;
    write_buffer.push_str(&state_attribute_values_lut);
    write_buffer.push_str("];\n\n");

    write_buffer.push_str(&shapes);

    write_buffer.push_str("pub static ALL_SHAPES: &[&'static [[f64; 6]]] = &[\n");
    for index in 0..shape_count {
        write_buffer.push_str(&format!("\t&SHAPE{},\n", index));
    }
    write_buffer.push_str("];\n");

    let mut f = crate::file_out("block_attribute_lut.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_u16_to_block_state(write_buffer: &mut String, state_count: usize, state_lut: Vec<String>) -> Result<(), anyhow::Error> {
    write_buffer.push_str("impl TryFrom<u16> for BlockState {\n");
    write_buffer.push_str("\ttype Error = NoSuchBlockError;");
    write_buffer.push_str("\tfn try_from(id: u16) -> Result<BlockState, Self::Error> {\n");
    write_buffer
        .push_str("\t\tif id >= BLOCK_LUT.len() as _ { return Err(NoSuchBlockError(id)); }\n");
    write_buffer.push_str("\t\tOk(BLOCK_LUT[id as usize])\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    writeln!(
        write_buffer,
        "const BLOCK_LUT: [BlockState; {}] = [",
        state_count
    )?;
    for element in state_lut {
        write_buffer.push_str(&element);
        write_buffer.push_str(",\n");
    }
    write_buffer.push_str("];");

    let mut f = crate::file_out("u16_to_block_state.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_u16_to_block_class(write_buffer: &mut String, state_count: usize, class_lut: Vec<String>) -> Result<(), anyhow::Error> {
    write_buffer.push_str("impl TryFrom<u16> for BlockClass {\n");
    write_buffer.push_str("\ttype Error = NoSuchBlockError;");
    write_buffer.push_str("\tfn try_from(id: u16) -> Result<BlockClass, Self::Error> {\n");
    write_buffer
        .push_str("\t\tif id >= BLOCK_CLASS_LUT.len() as _ { return Err(NoSuchBlockError(id)); }\n");
    write_buffer.push_str("\t\tOk(BLOCK_CLASS_LUT[id as usize])\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    writeln!(
        write_buffer,
        "const BLOCK_CLASS_LUT: [BlockClass; {}] = [",
        state_count
    )?;
    for element in class_lut {
        write_buffer.push_str(&element);
        write_buffer.push_str(",\n");
    }
    write_buffer.push_str("];");

    let mut f = crate::file_out("u16_to_block_class.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_block_state_to_u16(write_buffer: &mut String, u16_from_block_def: String) -> Result<(), anyhow::Error> {
    write_buffer.push_str("impl BlockState {\n");
    write_buffer.push_str("\tpub const fn to_id(&self) -> u16 {\n");
    write_buffer.push_str("\t\tmatch self {\n");
    write_buffer.push_str(&u16_from_block_def);
    write_buffer.push_str("\t\t_ => 0\n");
    write_buffer.push_str("\t\t}\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n");
    let mut f = crate::file_out("block_state_to_u16.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_block_state_id_to_block(write_buffer: &mut String, state_count: usize, block_state_id_to_block: String) -> Result<(), anyhow::Error> {
    writeln!(
        write_buffer,
        "const BLOCK_STATE_ID_TO_BLOCK: [Block; {}] = [",
        state_count
    )?;
    write_buffer.push_str(&block_state_id_to_block);
    write_buffer.push_str("];\n\n");

    let mut f = crate::file_out("block_state_id_to_block.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_string_to_u16(write_buffer: &mut String, string_to_u16_def: phf_codegen::Map<String>) -> Result<(), anyhow::Error> {
    write!(write_buffer, "static STRING_TO_U16: phf::Map<&'static str, u16> = {}", string_to_u16_def.build())?;
    write!(write_buffer, ";\n").unwrap();

    let mut f = crate::file_out("block_string_to_u16.rs");
    f.write_all(write_buffer.as_bytes())?;
    write_buffer.clear();
    Ok(())
}

fn write_set_block_property(write_buffer: &mut String, set_property_value_string: String) -> Result<(), anyhow::Error> {
    write_buffer.push_str("impl BlockState {\n");
    write_buffer.push_str("pub fn set_property(self, name: &str, value: &str) -> Option<BlockState> {\n");
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
}

fn write_state_attributes(state_attributes_lut: &mut String, state_attribute_values_lut: &mut String,
        shapes: &mut String, state_attribute_lookup: &mut Vec<ResolvedBlockAttributes>, shape_lookup: &mut Vec<Vec<[f64; 6]>>,
        bounds_lookup: &mut Vec<[f64; 6]>,
        block_name: &String, state_id: usize, block: &Block) -> Result<(), anyhow::Error> {
    let mut fluid_state = block.attributes.fluid_state.unwrap_or(0);
    let mut hardness = block.attributes.hardness.unwrap_or(0.0);
    let mut friction = block.attributes.friction.unwrap_or(0.6_f32);
    let mut speed_factor = block.attributes.speed_factor.unwrap_or(1.0);
    let mut jump_factor = block.attributes.jump_factor.unwrap_or(1.0);
    let mut replaceable = block.attributes.replaceable.unwrap_or(false);
    let mut air = block.attributes.air.unwrap_or(false);
    let mut blocks_motion = block.attributes.blocks_motion.unwrap_or(true);
    let mut waterlogged = block.attributes.waterlogged.unwrap_or(false);
    let mut suffocating = block.attributes.suffocating.unwrap_or(false);
    let mut is_sturdy_north = block.attributes.is_sturdy_north.unwrap_or(true);
    let mut is_sturdy_east = block.attributes.is_sturdy_east.unwrap_or(true);
    let mut is_sturdy_south = block.attributes.is_sturdy_south.unwrap_or(true);
    let mut is_sturdy_west = block.attributes.is_sturdy_west.unwrap_or(true);
    let mut is_pathfindable_land = block.attributes.is_pathfindable_land.unwrap_or(false);
    let mut is_pathfindable_air = block.attributes.is_pathfindable_air.unwrap_or(false);
    let mut is_pathfindable_water = block.attributes.is_pathfindable_water.unwrap_or(false);
    let mut light_emission = block.attributes.light_emission.unwrap_or(0);
    let mut fall_damage_resetting = block.attributes.fall_damage_resetting.unwrap_or(false);
    let mut climbable = block.attributes.climbable.unwrap_or(false);
    let mut clockwise_rotation_state_id_offset = block.attributes.clockwise_rotation_state_id_offset.unwrap_or(0);
    let mut collision_shape = block.attributes.collision_shape.clone();
    let state_attributes = block.state_attributes.get(&state_id.to_string());

    if let Some(state_attributes) = state_attributes {
        fluid_state = state_attributes.fluid_state.unwrap_or(fluid_state);
        hardness = state_attributes.hardness.unwrap_or(hardness);
        friction = state_attributes.friction.unwrap_or(friction);
        speed_factor = state_attributes.speed_factor.unwrap_or(speed_factor);
        jump_factor = state_attributes.jump_factor.unwrap_or(jump_factor);
        replaceable = state_attributes.replaceable.unwrap_or(replaceable);
        air = state_attributes.air.unwrap_or(air);
        blocks_motion = state_attributes.blocks_motion.unwrap_or(blocks_motion);
        waterlogged = state_attributes.waterlogged.unwrap_or(waterlogged);
        suffocating = state_attributes.suffocating.unwrap_or(suffocating);
        is_sturdy_north = block.attributes.is_sturdy_north.unwrap_or(is_sturdy_north);
        is_sturdy_east = block.attributes.is_sturdy_east.unwrap_or(is_sturdy_east);
        is_sturdy_south = block.attributes.is_sturdy_south.unwrap_or(is_sturdy_south);
        is_sturdy_west = block.attributes.is_sturdy_west.unwrap_or(is_sturdy_west);
        is_pathfindable_land = state_attributes.is_pathfindable_land.unwrap_or(is_pathfindable_land);
        is_pathfindable_air = state_attributes.is_pathfindable_air.unwrap_or(is_pathfindable_air);
        is_pathfindable_water = state_attributes.is_pathfindable_water.unwrap_or(is_pathfindable_water);
        light_emission = state_attributes.light_emission.unwrap_or(light_emission);
        fall_damage_resetting = state_attributes.fall_damage_resetting.unwrap_or(fall_damage_resetting);
        climbable = state_attributes.climbable.unwrap_or(climbable);
        clockwise_rotation_state_id_offset = state_attributes.clockwise_rotation_state_id_offset.unwrap_or(clockwise_rotation_state_id_offset);

        if !state_attributes.collision_shape.is_empty() {
            collision_shape = state_attributes.collision_shape.clone();
        }
    }

    let collision_bounds = if collision_shape.is_empty() {
        None
    } else {
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut min_z = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        let mut max_z = f64::MIN;

        for shape in &collision_shape {
            min_x = min_x.min(shape[0]);
            min_y = min_y.min(shape[1]);
            min_z = min_z.min(shape[2]);
            max_x = max_x.max(shape[3]);
            max_y = max_y.max(shape[4]);
            max_z = max_z.max(shape[5]);
        }

        Some([min_x, min_y, min_z, max_x, max_y, max_z])
    };

    // Bounds
    let collision_bounds_index;
    if let Some(collision_bounds) = collision_bounds {
        collision_bounds_index = if let Some(bounds_index) = bounds_lookup.iter().position(|r| r == &collision_bounds) {
            Some(bounds_index)
        } else {
            writeln!(
                shapes,
                "const BOUNDS{}: [f64; 6] = [{}_f64, {}_f64, {}_f64, {}_f64, {}_f64, {}_f64]; // eg. {} ({})",
                bounds_lookup.len(),
                collision_bounds[0], collision_bounds[1], collision_bounds[2],
                collision_bounds[3], collision_bounds[4], collision_bounds[5],
                block_name, state_id, 
            )?;
    
            bounds_lookup.push(collision_bounds);
            Some(bounds_lookup.len() - 1)
        };
    } else {
        collision_bounds_index = None;
    }

    // Shape
    let collision_shape_index = if let Some(shape_index) = shape_lookup.iter().position(|r| r == &collision_shape) {
        shape_index
    } else {
        writeln!(
            shapes,
            "const SHAPE{}: [[f64; 6]; {}] = [ // eg. {} ({})",
            shape_lookup.len(), collision_shape.len(),
            block_name, state_id
        )?;

        for aabb in &collision_shape {
            writeln!(
                shapes,
                "\t[{}_f64, {}_f64, {}_f64, {}_f64, {}_f64, {}_f64],",
                aabb[0], aabb[1], aabb[2],
                aabb[3], aabb[4], aabb[5], 
            )?;
        }

        writeln!(shapes, "];")?;

        shape_lookup.push(collision_shape);
        shape_lookup.len() - 1
    };

    let resolved = ResolvedBlockAttributes {
        fluid_state,
        hardness,
        friction,
        speed_factor,
        jump_factor,
        replaceable,
        air,
        blocks_motion,
        waterlogged,
        suffocating,
        is_sturdy_north,
        is_sturdy_east,
        is_sturdy_south,
        is_sturdy_west,
        is_pathfindable_land,
        is_pathfindable_air,
        is_pathfindable_water,
        light_emission,
        fall_damage_resetting,
        climbable,
        clockwise_rotation_state_id_offset,
        collision_bounds: collision_bounds_index,
        collision_shape: collision_shape_index
    };

    let index = if let Some(index) = state_attribute_lookup.iter().position(|r| r == &resolved) {
        index
    } else {
        writeln!(
            state_attribute_values_lut,
            "\tBlockAttributes {{ // eg. {} ({})",
            block_name, state_id
        )?;

        let mut flags = Vec::new();

        if replaceable {
            flags.push("Replaceable");
        }
        if air {
            flags.push("Air");
        }
        if blocks_motion {
            flags.push("BlocksMotion");
        }
        if waterlogged {
            flags.push("Waterlogged");
        }
        if suffocating {
            flags.push("Suffocating");
        }
        if is_sturdy_north {
            flags.push("IsSturdyNorth");
        }
        if is_sturdy_east {
            flags.push("IsSturdyEast");
        }
        if is_sturdy_south {
            flags.push("IsSturdySouth");
        }
        if is_sturdy_west {
            flags.push("IsSturdyWest");
        }
        if is_pathfindable_land {
            flags.push("IsPathfindableLand");
        }
        if is_pathfindable_air {
            flags.push("IsPathfindableAir");
        }
        if is_pathfindable_water {
            flags.push("IsPathfindableWater");
        }
        if fall_damage_resetting {
            flags.push("FallDamageResetting");
        }
        if climbable {
            flags.push("Climbable");
        }

        if flags.is_empty() {
            state_attribute_values_lut.push_str("\t\tflags: enumset::EnumSet::empty(),");
        } else {
            state_attribute_values_lut.push_str("\t\tflags: enumset::enum_set_union!(");
            for flag in flags {
                state_attribute_values_lut.push_str("BlockFlag::");
                state_attribute_values_lut.push_str(flag);
                state_attribute_values_lut.push_str(", ");
            }
            state_attribute_values_lut.push_str("),\n");
        }

        writeln!(state_attribute_values_lut, "\t\tfluid_state: {},", fluid_state)?;
        writeln!(state_attribute_values_lut, "\t\thardness: {}_f32,", hardness)?;
        writeln!(state_attribute_values_lut, "\t\tfriction: {}_f32,", friction)?;
        writeln!(state_attribute_values_lut, "\t\tspeed_factor: {}_f32,", speed_factor)?;
        writeln!(state_attribute_values_lut, "\t\tjump_factor: {}_f32,", jump_factor)?;
        // writeln!(state_attribute_values_lut, "\t\treplaceable: {},", replaceable)?;
        // writeln!(state_attribute_values_lut, "\t\tair: {},", air)?;
        // writeln!(state_attribute_values_lut, "\t\tblocks_motion: {},", blocks_motion)?;
        // writeln!(state_attribute_values_lut, "\t\twaterlogged: {},", waterlogged)?;
        // writeln!(state_attribute_values_lut, "\t\tis_sturdy_north: {},", is_sturdy_north)?;
        // writeln!(state_attribute_values_lut, "\t\tis_sturdy_east: {},", is_sturdy_east)?;
        // writeln!(state_attribute_values_lut, "\t\tis_sturdy_south: {},", is_sturdy_south)?;
        // writeln!(state_attribute_values_lut, "\t\tis_sturdy_west: {},", is_sturdy_west)?;
        // writeln!(state_attribute_values_lut, "\t\tis_pathfindable_land: {},", is_pathfindable_land)?;
        // writeln!(state_attribute_values_lut, "\t\tis_pathfindable_air: {},", is_pathfindable_air)?;
        // writeln!(state_attribute_values_lut, "\t\tis_pathfindable_water: {},", is_pathfindable_water)?;
        writeln!(state_attribute_values_lut, "\t\tlight_emission: {},", light_emission)?;
        // writeln!(state_attribute_values_lut, "\t\tfall_damage_resetting: {},", fall_damage_resetting)?;
        // writeln!(state_attribute_values_lut, "\t\tclimbable: {},", climbable)?;
        writeln!(state_attribute_values_lut, "\t\tclockwise_rotation_state_id_offset: {},", clockwise_rotation_state_id_offset)?;
        if let Some(bounds_index) = collision_bounds_index {
            writeln!(state_attribute_values_lut, "\t\tcollision_bounds: Some(&BOUNDS{}),", bounds_index)?;
        } else {
            writeln!(state_attribute_values_lut, "\t\tcollision_bounds: None,")?;
        }
        writeln!(state_attribute_values_lut, "\t\tcollision_shape: &SHAPE{},", collision_shape_index)?;
        writeln!(state_attribute_values_lut, "\t\tcollision_shape_id: {},", collision_shape_index)?;
    
        state_attribute_values_lut.push_str("\t},\n");

        state_attribute_lookup.push(resolved);
        state_attribute_lookup.len() - 1
    };
    
    writeln!(
        state_attributes_lut,
        "\t{}, // {} ({})",
        index, block_name, state_id
    )?;

    Ok(())
}

fn write_block_state(
    block_state_def: &mut String,
    block_class_def: &mut String,
    used_classes: &mut HashSet<String>,
    state_lut: &mut Vec<String>,
    class_lut: &mut Vec<String>,
    u16_from_block_def: &mut String,
    set_property_value_string: &mut String,
    parameters: &PropertyWriter,
    block_name: &str,
    block: &Block,
    current_state_id: usize,
) -> anyhow::Result<usize> {
    let mut all_possible_parameters = Vec::new();

    block_state_def.push('\t');
    block_state_def.push_str(&block_name.to_case(Case::Pascal));

    let should_write_class = used_classes.insert(block.class.clone());

    if should_write_class {
        block_class_def.push('\t');
        block_class_def.push_str(&block.class.to_case(Case::Pascal));
    }

    let block_enum_ref = format!("BlockState::{}", block_name.to_case(Case::Pascal));

    if block.properties.is_empty() {
        block_state_def.push_str(" {},\n");
        if should_write_class {
            block_class_def.push_str(" {},\n");
        }

        writeln!(u16_from_block_def, "\t\t\t{} {{}} => {},", block_enum_ref, current_state_id)?;

        while state_lut.len() <= current_state_id {
            state_lut.push(String::new());
        }
        state_lut[current_state_id] = format!("\t{} {{}}", block_enum_ref);

        while class_lut.len() <= current_state_id {
            class_lut.push(String::new());
        }
        class_lut[current_state_id] = format!("\tBlockClass::{} {{}}", block.class.to_case(Case::Pascal));

        return Ok(1);
    } else {
        block_state_def.push_str(" {\n");
        if should_write_class {
            block_class_def.push_str(" {\n");
        }

        // Emit eg. "BlockState::AcaciaButton{face, facing, powered} => {" for set_property method
        set_property_value_string.push_str("\t\tBlockState::");
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

                    writeln!(block_state_def, "\t\t{field_name}: {parameter_name},")?;
                    if should_write_class {
                        writeln!(block_class_def, "\t\t{field_name}: {parameter_name},")?;
                    }

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

                    block_state_def.push_str("\t\t");
                    block_state_def.push_str(field_name);
                    block_state_def.push_str(": bool,\n");

                    if should_write_class {
                        block_class_def.push_str("\t\t");
                        block_class_def.push_str(field_name);
                        block_class_def.push_str(": bool,\n");
                    }
                }
                Property::Int {
                    values,
                    default_value: _,
                } => {
                    block_state_def.push_str("\t\t");
                    block_state_def.push_str(field_name);
                    block_state_def.push_str(": u8,\n");

                    if should_write_class {
                        block_class_def.push_str("\t\t");
                        block_class_def.push_str(field_name);
                        block_class_def.push_str(": u8,\n");
                    }

                    let mut named_values = Vec::new();
                    for value in values {
                        named_values.push(format!("{field_name}: {value},"));
                    }
                    all_possible_parameters.push(named_values);
                }
            }
        }
        block_state_def.push_str("\t},\n");
        if should_write_class {
            block_class_def.push_str("\t},\n");
        }
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
        state_def.push_str("\tBlockState::");
        state_def.push_str(&block_name.to_case(Case::Pascal));
        state_def.push('{');
        state_def.push_str(&one);
        state_def.push('}');

        // Push into LUT
        while state_lut.len() <= index {
            state_lut.push(String::new());
        }
        state_lut[index] = state_def.clone();

        while class_lut.len() <= index {
            class_lut.push(String::new());
        }
        class_lut[index] = format!("\tBlockClass::{} {{ {} }}", block.class.to_case(Case::Pascal), one);

        // Push into From
        writeln!(u16_from_block_def, "\t\t{state_def} => {index},")?;

        index += 1;
    }

    Ok(all_count)
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