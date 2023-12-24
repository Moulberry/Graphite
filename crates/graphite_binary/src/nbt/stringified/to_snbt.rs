use crate::nbt::*;
use std::fmt::Write;

pub fn to_snbt_string(nbt: &NBT) -> String {
    let mut snbt = String::new();
    to_snbt(&mut snbt, nbt).expect("string writing is infallible");
    snbt
}

pub fn to_snbt<T: Write>(writer: &mut T, nbt: &NBT) -> std::fmt::Result {
    write_compound(writer, &nbt.nodes, &nbt.root_children)
}

fn write_node<T: Write>(writer: &mut T, nodes: &Vec<NBTNode>, node: &NBTNode) -> std::fmt::Result {
    match node {
        NBTNode::Byte(value) => write_byte(writer, *value),
        NBTNode::Short(value) => write_short(writer, *value),
        NBTNode::Int(value) => write_int(writer, *value),
        NBTNode::Long(value) => write_long(writer, *value),
        NBTNode::Float(value) => write_float(writer, *value),
        NBTNode::Double(value) => write_double(writer, *value),
        NBTNode::ByteArray(values) => write_byte_array(writer, values),
        NBTNode::String(value) => write_string(writer, value),
        NBTNode::List {
            type_id: _,
            children,
        } => write_list(writer, children, nodes),
        NBTNode::Compound(value) => write_compound(writer, nodes, value),
        NBTNode::IntArray(values) => write_int_array(writer, values),
        NBTNode::LongArray(values) => write_long_array(writer, values),
    }
}

fn write_compound<T: Write>(
    writer: &mut T,
    nodes: &Vec<NBTNode>,
    children: &NBTCompound,
) -> std::fmt::Result {
    writer.write_char('{')?;

    let mut first = true;

    for (child_name, child_idx) in &children.0 {
        if first {
            first = false;
        } else {
            writer.write_str(", ")?;
        }

        write_key(writer, child_name)?;
        writer.write_str(": ")?;

        let child = &nodes[*child_idx];
        write_node(writer, nodes, child)?;
    }

    writer.write_char('}')?;
    Ok(())
}

fn write_key<T: Write>(writer: &mut T, value: &str) -> std::fmt::Result {
    // String must match `[A-Za-z0-9._+-]+` to be unquoted
    for c in value.chars() {
        if matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z' | '.' | '_' | '+' | '-') {
            // Contains invalid character, write a quoted string instead
            return write_string(writer, value);
        }
    }

    // All good to go - write the unquoted string
    writer.write_str(value)
}

fn write_string<T: Write>(writer: &mut T, value: &str) -> std::fmt::Result {
    writer.write_char('"')?;

    for c in value.chars() {
        // Escape backslashes and quotes
        if c == '\\' || c == '"' {
            writer.write_char('\\')?;
        }
        // Push the char
        writer.write_char(c)?;
    }

    writer.write_char('"')
}

// Note: doing write!("{}") and then push('b')
// is about 25% faster than doing write!("{}b")

fn write_byte<T: Write>(writer: &mut T, value: i8) -> std::fmt::Result {
    write!(writer, "{}", value)?;
    writer.write_char('b')
}

fn write_short<T: Write>(writer: &mut T, value: i16) -> std::fmt::Result {
    write!(writer, "{}", value)?;
    writer.write_char('s')
}

fn write_int<T: Write>(writer: &mut T, value: i32) -> std::fmt::Result {
    write!(writer, "{}", value)
}

fn write_long<T: Write>(writer: &mut T, value: i64) -> std::fmt::Result {
    write!(writer, "{}", value)?;
    writer.write_char('L')
}

fn write_float<T: Write>(writer: &mut T, value: f32) -> std::fmt::Result {
    write!(writer, "{}", value)?;
    writer.write_char('f')
}

fn write_double<T: Write>(writer: &mut T, value: f64) -> std::fmt::Result {
    write!(writer, "{}", value)?;
    writer.write_char('d')
}

fn write_byte_array<T: Write>(writer: &mut T, values: &Vec<i8>) -> std::fmt::Result {
    writer.write_str("[B;")?;
    let mut first = true;
    for byte in values {
        if first {
            first = false;
        } else {
            writer.write_char(',')?;
        }
        write_byte(writer, *byte)?;
    }
    writer.write_char(']')
}

fn write_list<T: Write>(
    writer: &mut T,
    children: &Vec<usize>,
    nodes: &Vec<NBTNode>,
) -> std::fmt::Result {
    writer.write_str("[")?;
    let mut first = true;
    for child in children {
        if first {
            first = false;
        } else {
            writer.write_str(", ")?;
        }

        let child = &nodes[*child];
        write_node(writer, nodes, child)?;
    }
    writer.write_char(']')
}

fn write_int_array<T: Write>(writer: &mut T, values: &Vec<i32>) -> std::fmt::Result {
    writer.write_str("[I;")?;
    let mut first = true;
    for int in values {
        if first {
            first = false;
        } else {
            writer.write_char(',')?;
        }
        write_int(writer, *int)?;
    }
    writer.write_char(']')
}

fn write_long_array<T: Write>(writer: &mut T, values: &Vec<i64>) -> std::fmt::Result {
    writer.write_str("[L;")?;
    let mut first = true;
    for long in values {
        if first {
            first = false;
        } else {
            writer.write_char(',')?;
        }
        write_long(writer, *long)?;
    }
    writer.write_char(']')
}
