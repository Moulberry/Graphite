use std::fmt::Write as _;
use std::io::Write;

use convert_case::{Case, Casing};
use indexmap::IndexMap;
use serde_derive::Deserialize;

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: usize,
    #[serde(default = "get_default_max_stack_size")]
    pub max_stack_size: u8,
    #[serde(default)]
    pub corresponding_block: String,
    #[serde(default = "get_default_use_duration")]
    pub use_duration: u32,
}

fn get_default_max_stack_size() -> u8 {
    64
}

fn get_default_use_duration() -> u32 {
    0
}

pub fn write_items() -> anyhow::Result<()> {
    let raw_data = include_str!("../data/items.json");
    let items: IndexMap<String, Item> = serde_json::from_str(raw_data)?;
    let item_count = items.len();

    let mut write_buffer = String::new();

    // Item Enum
    write_buffer.push_str("#[derive(Debug, Clone, Copy, Eq, PartialEq)]\n");
    write_buffer.push_str("#[repr(u16)]\n");
    write_buffer.push_str("pub enum Item {\n");
    for (item_name, _) in &items {
        writeln!(write_buffer, "\t{},", item_name.to_case(Case::Pascal))?;
    }
    write_buffer.push_str("}\n\n");

    write_buffer.push_str(
        r#"impl Item {
    pub fn get_properties(self) -> &'static ItemProperties {
        &ITEM_PROPERTIES_LUT[self as usize]
    }
}"#,
    );

    write_buffer.push_str("\n\n");

    // Item Properties Struct
    write_buffer.push_str("#[derive(Debug)]\n");
    write_buffer.push_str("pub struct ItemProperties {\n");
    write_buffer.push_str("\tpub max_stack_size: u8,\n");
    write_buffer.push_str("\tpub use_duration: u32,\n");
    write_buffer.push_str("\tpub has_corresponding_block: bool,\n");
    write_buffer.push_str("}\n\n");

    writeln!(
        write_buffer,
        "const ITEM_PROPERTIES_LUT: [ItemProperties; {}] = [",
        item_count
    )?;
    for (item_name, item) in &items {
        writeln!(write_buffer, "\tItemProperties {{ // {}", item_name)?;
        writeln!(write_buffer, "\t\tmax_stack_size: {},", item.max_stack_size)?;
        writeln!(write_buffer, "\t\tuse_duration: {},", item.use_duration)?;
        writeln!(
            write_buffer,
            "\t\thas_corresponding_block: {},",
            !item.corresponding_block.is_empty()
        )?;
        write_buffer.push_str("\t},\n");
    }
    write_buffer.push_str("];\n\n");

    // NoSuchItemError
    write_buffer.push_str("#[derive(Debug, thiserror::Error)]\n");
    write_buffer.push_str("#[error(\"No item exists for id: {0}\")]\n");
    write_buffer.push_str("pub struct NoSuchItemError(u16);\n\n");

    // TryFrom<u16> for Item
    write_buffer.push_str("impl TryFrom<u16> for Item {\n");
    write_buffer.push_str("\ttype Error = NoSuchItemError;\n");
    write_buffer.push_str("\tfn try_from(value: u16) -> Result<Self, Self::Error> {\n");
    writeln!(
        write_buffer,
        "\t\tif value >= {} {{ return Err(NoSuchItemError(value)); }}",
        item_count
    )?;
    write_buffer.push_str("\t\tOk(unsafe { std::mem::transmute(value) })\n");
    write_buffer.push_str("\t}\n");
    write_buffer.push_str("}\n\n");

    let mut f = crate::file_src("item.rs");
    f.write_all(write_buffer.as_bytes())?;

    Ok(())
}
