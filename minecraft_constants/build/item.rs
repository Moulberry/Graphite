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
    write_buffer.push_str("#[repr(u16)]\n");
    write_buffer.push_str("pub enum Item {\n");

    for item in items {
        writeln!(write_buffer, "\t{},", item.name.to_case(Case::Pascal))?;
    }

    write_buffer.push('}');

    let mut f = crate::file_src("item.rs");
    f.write_all(write_buffer.as_bytes())?;

    Ok(())
}
