use crate::nbt::*;
use std::fmt::Write;

pub fn to_pretty_debug<T: Write>(writer: &mut T, nbt: &NBT) -> std::fmt::Result {
    let mut indentation = String::new();
    write_compound(
        writer,
        &nbt.nodes,
        &mut indentation,
        Some(&nbt.root_name),
        &nbt.root_children,
    )
}

fn write_node<T: Write>(
    writer: &mut T,
    nodes: &Vec<NBTNode>,
    indentation: &mut String,
    name: Option<&String>,
    node: &NBTNode,
) -> std::fmt::Result {
    match node {
        NBTNode::Byte(value) => write_byte(writer, indentation, name, *value),
        NBTNode::Short(value) => write_short(writer, indentation, name, *value),
        NBTNode::Int(value) => write_int(writer, indentation, name, *value),
        NBTNode::Long(value) => write_long(writer, indentation, name, *value),
        NBTNode::Float(value) => write_float(writer, indentation, name, *value),
        NBTNode::Double(value) => write_double(writer, indentation, name, *value),
        NBTNode::ByteArray(values) => write_byte_array(writer, indentation, name, values),
        NBTNode::String(value) => write_string(writer, indentation, name, value),
        NBTNode::List {
            type_id: _,
            children,
        } => write_list(writer, nodes, indentation, name, children),
        NBTNode::Compound(value) => write_compound(writer, nodes, indentation, name, value),
        NBTNode::IntArray(values) => write_int_array(writer, indentation, name, values),
        NBTNode::LongArray(values) => write_long_array(writer, indentation, name, values),
    }
}

fn write_compound<T: Write>(
    writer: &mut T,
    nodes: &Vec<NBTNode>,
    indentation: &mut String,
    name: Option<&String>,
    children: &NBTCompound,
) -> std::fmt::Result {
    // Write type header and opening brace
    writer.write_str(indentation)?;
    if let Some(name) = name {
        write!(writer, "Compound('{}'): {} entries", name, children.0.len())?;
    } else {
        write!(writer, "Compound(None): {} entries", children.0.len())?;
    }
    if children.0.is_empty() {
        return Ok(());
    }
    writeln!(writer, "\n{}{{", indentation)?;

    // Increase indentation
    indentation.push_str("  ");

    for (child_name, child_idx) in &children.0 {
        let child = &nodes[*child_idx];
        write_node(writer, nodes, indentation, Some(child_name), child)?;
        writer.write_char('\n')?;
    }

    // Decrease indentation
    indentation.truncate(indentation.len() - 2);

    // Closing brace
    writer.write_str(indentation)?;
    writer.write_char('}')
}

fn write_string<T: Write>(
    writer: &mut T,
    indentation: &mut String,
    name: Option<&String>,
    value: &str,
) -> std::fmt::Result {
    writer.write_str(indentation)?;
    if let Some(name) = name {
        write!(writer, "String('{}'): '{}'", name, value)
    } else {
        write!(writer, "String(None): '{}'", value)
    }
}

fn write_byte<T: Write>(
    writer: &mut T,
    indentation: &mut String,
    name: Option<&String>,
    value: i8,
) -> std::fmt::Result {
    writer.write_str(indentation)?;
    if let Some(name) = name {
        write!(writer, "Byte('{}'): {}", name, value)
    } else {
        write!(writer, "Byte(None): {}", value)
    }
}

fn write_short<T: Write>(
    writer: &mut T,
    indentation: &mut String,
    name: Option<&String>,
    value: i16,
) -> std::fmt::Result {
    writer.write_str(indentation)?;
    if let Some(name) = name {
        write!(writer, "Short('{}'): {}", name, value)
    } else {
        write!(writer, "Short(None): {}", value)
    }
}

fn write_int<T: Write>(
    writer: &mut T,
    indentation: &mut String,
    name: Option<&String>,
    value: i32,
) -> std::fmt::Result {
    writer.write_str(indentation)?;
    if let Some(name) = name {
        write!(writer, "Int('{}'): {}", name, value)
    } else {
        write!(writer, "Int(None): {}", value)
    }
}

fn write_long<T: Write>(
    writer: &mut T,
    indentation: &mut String,
    name: Option<&String>,
    value: i64,
) -> std::fmt::Result {
    writer.write_str(indentation)?;
    if let Some(name) = name {
        write!(writer, "Long('{}'): {}", name, value)
    } else {
        write!(writer, "Long(None): {}", value)
    }
}

fn write_float<T: Write>(
    writer: &mut T,
    indentation: &mut String,
    name: Option<&String>,
    value: f32,
) -> std::fmt::Result {
    writer.write_str(indentation)?;
    if let Some(name) = name {
        write!(writer, "Float('{}'): {}", name, value)
    } else {
        write!(writer, "Float(None): {}", value)
    }
}

fn write_double<T: Write>(
    writer: &mut T,
    indentation: &mut String,
    name: Option<&String>,
    value: f64,
) -> std::fmt::Result {
    writer.write_str(indentation)?;
    if let Some(name) = name {
        write!(writer, "Double('{}'): {}", name, value)
    } else {
        write!(writer, "Double(None): {}", value)
    }
}

fn write_list<T: Write>(
    writer: &mut T,
    nodes: &Vec<NBTNode>,
    indentation: &mut String,
    name: Option<&String>,
    children: &Vec<usize>,
) -> std::fmt::Result {
    // Write type header and opening bracket
    writer.write_str(indentation)?;
    if let Some(name) = name {
        write!(writer, "List('{}'): {} entries", name, children.len())?;
    } else {
        write!(writer, "List(None): {} entries", children.len())?;
    }
    if children.is_empty() {
        return Ok(());
    }
    writeln!(writer, "\n{}[", indentation)?;

    // Increase indentation
    indentation.push_str("  ");

    for child in children {
        let child = &nodes[*child];
        write_node(writer, nodes, indentation, None, child)?;
        writer.write_char('\n')?;
    }

    // Decrease indentation
    indentation.truncate(indentation.len() - 2);

    // Closing bracket
    writer.write_str(indentation)?;
    writer.write_char(']')
}

fn write_byte_array<T: Write>(
    writer: &mut T,
    indentation: &String,
    name: Option<&String>,
    values: &Vec<i8>,
) -> std::fmt::Result {
    writer.write_str(indentation)?;
    if let Some(name) = name {
        write!(writer, "ByteArray('{}'): [", name)?;
    } else {
        writer.write_str("ByteArray(None): [")?;
    }

    if values.len() > 16 {
        write!(writer, "{} bytes]", values.len())
    } else {
        let mut first = true;
        for byte in values {
            if first {
                first = false;
                write!(writer, "{}", *byte)?;
            } else {
                write!(writer, ", {}", *byte)?;
            }
        }
        writer.write_char(']')
    }
}

fn write_int_array<T: Write>(
    writer: &mut T,
    indentation: &String,
    name: Option<&String>,
    values: &Vec<i32>,
) -> std::fmt::Result {
    writer.write_str(indentation)?;
    if let Some(name) = name {
        write!(writer, "IntArray('{}'): [", name)?;
    } else {
        writer.write_str("IntArray(None): [")?;
    }

    if values.len() > 16 {
        write!(writer, "{} ints]", values.len())
    } else {
        let mut first = true;
        for byte in values {
            if first {
                first = false;
                write!(writer, "{}", *byte)?;
            } else {
                write!(writer, ", {}", *byte)?;
            }
        }
        writer.write_char(']')
    }
}

fn write_long_array<T: Write>(
    writer: &mut T,
    indentation: &String,
    name: Option<&String>,
    values: &Vec<i64>,
) -> std::fmt::Result {
    writer.write_str(indentation)?;
    if let Some(name) = name {
        write!(writer, "LongArray('{}'): [", name)?;
    } else {
        writer.write_str("LongArray(None): [")?;
    }

    if values.len() > 16 {
        write!(writer, "{} longs]", values.len())
    } else {
        let mut first = true;
        for byte in values {
            if first {
                first = false;
                write!(writer, "{}", *byte)?;
            } else {
                write!(writer, ", {}", *byte)?;
            }
        }
        writer.write_char(']')
    }
}
