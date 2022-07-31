use std::fmt::Write as _;
use std::io::Write;

use convert_case::{Case, Casing};
use serde_derive::Deserialize;

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    id: usize,
    name: &'static str,
    display_name: &'static str,
    stack_size: u8,
}

pub fn write_items() -> anyhow::Result<()> {
    let raw_data = include_str!("../minecraft-data/data/pc/1.19/items.json");
    let items: Vec<Item> = serde_json::from_str(raw_data)?;

    let mut write_buffer = String::new();
    // write_buffer.push_str("#[derive(num_enum::TryFromPrimitive)]\n");
    write_buffer.push_str("#[derive(Debug, Clone, Copy, Eq, PartialEq)]\n");
    write_buffer.push_str("#[repr(u16)]\n");
    write_buffer.push_str("pub enum Item {\n");

    let item_count = items.len();
    for item in items {
        writeln!(write_buffer, "\t{},", item.name.to_case(Case::Pascal))?;
    }

    write_buffer.push_str("}\n\n");

    write_buffer.push_str("#[derive(Debug, thiserror::Error)]\n");
    write_buffer.push_str("#[error(\"No item exists for id: {0}\")]\n");
    write_buffer.push_str("pub struct NoSuchItemError(u16);\n\n");
    
    write_buffer.push_str("impl TryFrom<u16> for Item {\n");
    write_buffer.push_str("\ttype Error = NoSuchItemError;\n");
    write_buffer.push_str("\tfn try_from(value: u16) -> Result<Self, Self::Error> {\n");
    writeln!(write_buffer, "\t\tif value >= {} {{ return Err(NoSuchItemError(value)); }}", item_count)?;
    write_buffer.push_str("\t\tOk(unsafe { std::mem::transmute(value) })\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}");

    let mut f = crate::file_src("item.rs");
    f.write_all(write_buffer.as_bytes())?;

    Ok(())
}
