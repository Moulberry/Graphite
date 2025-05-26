use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write;

use indexmap::IndexMap;

pub fn write_font_widths() -> anyhow::Result<()> {
    let raw_data = include_str!("../data/font_width_map.json");
    let widths: IndexMap<usize, Vec<usize>> = serde_json::from_str(raw_data)?;

    let mut map = HashMap::new();
    let mut max_char = 0;

    for (width, chars) in widths {
        for char in chars {
            map.insert(char, width);
            max_char = max_char.max(char);
        }
    }

    let mut write_buffer = String::new();

    write_buffer.push_str(r#"
pub fn width_of_string(string: &str) -> isize {
    let mut sum = 0;
    for char in string.chars() {
        sum += width_of_char(char)
    }
    sum
}

pub fn width_of_char(character: char) -> isize {
    let index = character as usize;
    if index >= WIDTHS.len() {
        6
    } else {
        WIDTHS[index] as isize
    }
}"#);

    write!(write_buffer, "\n\nstatic WIDTHS: [u8; {}] = [", max_char)?;
    for i in 0..max_char {
        let width = *map.get(&i).unwrap_or(&6);
        if i % 32 == 0 {
            write_buffer.push_str("\n\t");
        }
        write!(write_buffer, "{}, ", width)?;
    }
    writeln!(write_buffer, "\n];")?;

    let mut f = crate::file_src("font_width.rs");
    f.write_all(write_buffer.as_bytes())?;

    Ok(())
}