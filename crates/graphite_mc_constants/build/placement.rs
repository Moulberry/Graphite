use crate::block::{Block, Property};
use crate::item::Item;
use convert_case::{Case, Casing};
use indexmap::IndexMap;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write;

pub fn write_placement(
    placement_method_returns: IndexMap<String, String>,
    aliases: IndexMap<(String, Vec<String>), String>,
) -> anyhow::Result<()> {
    let mut write_buffer = String::new();
    let block_type_str = String::from("block_type");

    let raw_data = include_str!("../data/blocks.json");
    let blocks: IndexMap<String, Block> = serde_json::from_str(raw_data)?;

    write_buffer.push_str("use crate::block_parameter::*;\n\n");

    write_buffer.push_str("use crate::item::Item;\n\n");
    write_buffer.push_str("use crate::block::Block;\n\n");

    // PlacementContext trait
    let mut generated_methods = BTreeMap::new();
    for (_, block) in &blocks {
        for (_, property) in &block.properties {
            match property {
                Property::Int {
                    values: _,
                    default_value: _,
                    placement_value: _,
                    placement_method,
                } => {
                    if let Some(placement_method) = placement_method {
                        generated_methods.insert(placement_method, "u8".into());
                    }
                }
                Property::Bool {
                    default_value: _,
                    placement_value: _,
                    placement_method,
                } => {
                    if let Some(placement_method) = placement_method {
                        generated_methods.insert(placement_method, "bool".into());
                    }
                }
                Property::String {
                    values: _,
                    default_value: _,
                    placement_value: _,
                    placement_method,
                } => {
                    if let Some(placement_method) = placement_method {
                        let return_type = placement_method_returns.get(placement_method).unwrap();
                        let return_type = return_type.to_case(Case::Pascal);

                        generated_methods.insert(placement_method, return_type);
                    }
                }
            }
        }
    }

    write_buffer.push_str("pub trait PlacementContext {\n");
    for (method_name, method_return) in generated_methods {
        writeln!(
            write_buffer,
            "\tfn {}(&mut self) -> {};",
            method_name, method_return
        )?;
    }
    write_buffer.push_str("}\n\n");

    let raw_data = include_str!("../data/items.json");
    let items: IndexMap<String, Item> = serde_json::from_str(raw_data)?;

    // try_place
    write_buffer.push_str("impl Item {\n");
    write_buffer.push_str(
        "\tpub fn try_place(&self, ctx: &mut impl PlacementContext) -> Option<Block> {\n",
    );
    write_buffer.push_str("\t\tmatch self {\n");
    for (item_name, item) in items {
        if !item.corresponding_block.is_empty() {
            let block_name = item.corresponding_block.replace("minecraft:", "");
            let item_name_pascal = item_name.to_case(Case::Pascal);
            let block_name_pascal = block_name.to_case(Case::Pascal);

            let block = blocks.get(&block_name).unwrap();

            if block.properties.is_empty() {
                writeln!(
                    write_buffer,
                    "\t\t\tItem::{} => Some(Block::{}),",
                    item_name_pascal, block_name_pascal
                )?;
            } else {
                writeln!(
                    write_buffer,
                    "\t\t\tItem::{} => Some(Block::{} {{",
                    item_name_pascal, block_name_pascal
                )?;
                for (property_name, property) in &block.properties {
                    let mut field_name = property_name;
                    if field_name.as_str() == "type" {
                        field_name = &block_type_str;
                    }

                    match property {
                        Property::Int {
                            values: _,
                            default_value,
                            placement_value,
                            placement_method,
                        } => {
                            if let Some(placement_method) = placement_method {
                                writeln!(
                                    write_buffer,
                                    "\t\t\t\t{}: ctx.{}(),",
                                    field_name, placement_method
                                )?;
                            } else {
                                let placement_value = if let Some(placement_value) = placement_value
                                {
                                    *placement_value
                                } else {
                                    *default_value
                                };

                                writeln!(
                                    write_buffer,
                                    "\t\t\t\t{}: {},",
                                    field_name, placement_value
                                )?;
                            }
                        }
                        Property::Bool {
                            default_value,
                            placement_value,
                            placement_method,
                        } => {
                            if let Some(placement_method) = placement_method {
                                writeln!(
                                    write_buffer,
                                    "\t\t\t\t{}: ctx.{}(),",
                                    field_name, placement_method
                                )?;
                            } else {
                                let placement_value = if let Some(placement_value) = placement_value
                                {
                                    *placement_value
                                } else {
                                    *default_value
                                };

                                writeln!(
                                    write_buffer,
                                    "\t\t\t\t{}: {},",
                                    field_name, placement_value
                                )?;
                            }
                        }
                        Property::String {
                            values,
                            default_value,
                            placement_value,
                            placement_method,
                        } => {
                            if let Some(placement_method) = placement_method {
                                writeln!(
                                    write_buffer,
                                    "\t\t\t\t{}: ctx.{}(),",
                                    field_name, placement_method
                                )?;
                            } else {
                                let placement_value = if let Some(placement_value) = placement_value
                                {
                                    placement_value
                                } else {
                                    default_value
                                };

                                let property_name_pascal = if let Some(name) =
                                    aliases.get(&(property_name.clone(), values.to_owned()))
                                {
                                    name.clone()
                                } else {
                                    property_name.to_case(Case::Pascal)
                                };
                                let placement_value_pascal = placement_value.to_case(Case::Pascal);
                                writeln!(
                                    write_buffer,
                                    "\t\t\t\t{}: {}::{},",
                                    field_name, property_name_pascal, placement_value_pascal
                                )?;
                            }
                        }
                    }
                }
                write_buffer.push_str("\t\t\t}),\n");
            }

            //write_buffer.push_str("Some(Block::)")
        }
    }
    write_buffer.push_str("\t\t\t_ => None\n");
    write_buffer.push_str("\t\t}\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n\n");

    let mut f = crate::file_src("placement.rs");
    f.write_all(write_buffer.as_bytes())?;

    Ok(())
}
