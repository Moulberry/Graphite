use super::*;

use bytes::BufMut;

pub fn write_named(nbt: &NBT) -> Vec<u8> {
    let mut vec = Vec::new();
    write_named_into(nbt, &mut vec);
    vec
}

pub fn write_named_into(nbt: &NBT, vec: &mut Vec<u8>) {
    vec.push(TAG_COMPOUND_ID.0);
    write_string(vec, &nbt.root_name);
    write_compound(vec, &nbt.nodes, &nbt.root_children);
}

pub fn write_any(nbt: &NBT) -> Vec<u8> {
    let mut vec = Vec::new();
    write_any_into(nbt, &mut vec);
    vec
}

pub fn write_any_into(nbt: &NBT, vec: &mut Vec<u8>) {
    vec.push(TAG_COMPOUND_ID.0);
    write_compound(vec, &nbt.nodes, &nbt.root_children);
}

fn write_node(vec: &mut Vec<u8>, nodes: &Vec<NBTNode>, name: Option<&str>, node: &NBTNode) {
    match node {
        NBTNode::Byte(value) => {
            if let Some(name) = name {
                vec.push(TAG_BYTE_ID.0);
                write_string(vec, name);
            }
            vec.put_i8(*value);
        }
        NBTNode::Short(value) => {
            if let Some(name) = name {
                vec.push(TAG_SHORT_ID.0);
                write_string(vec, name);
            }
            vec.put_i16(*value);
        }
        NBTNode::Int(value) => {
            if let Some(name) = name {
                vec.push(TAG_INT_ID.0);
                write_string(vec, name);
            }
            vec.put_i32(*value);
        }
        NBTNode::Long(value) => {
            if let Some(name) = name {
                vec.push(TAG_LONG_ID.0);
                write_string(vec, name);
            }
            vec.put_i64(*value);
        }
        NBTNode::Float(value) => {
            if let Some(name) = name {
                vec.push(TAG_FLOAT_ID.0);
                write_string(vec, name);
            }
            vec.put_f32(*value);
        }
        NBTNode::Double(value) => {
            if let Some(name) = name {
                vec.push(TAG_DOUBLE_ID.0);
                write_string(vec, name);
            }
            vec.put_f64(*value);
        }
        NBTNode::ByteArray(values) => {
            if let Some(name) = name {
                vec.push(TAG_BYTE_ARRAY_ID.0);
                write_string(vec, name);
            }
            vec.put_i32(values.len() as _);
            vec.extend_from_slice(unsafe { std::mem::transmute(values.as_slice()) });
        }
        NBTNode::String(value) => {
            if let Some(name) = name {
                vec.push(TAG_STRING_ID.0);
                write_string(vec, name);
            }
            write_string(vec, value);
        }
        NBTNode::List { type_id, children } => {
            if let Some(name) = name {
                vec.push(TAG_LIST_ID.0);
                write_string(vec, name);
            }
            vec.push(type_id.0);
            vec.put_i32(children.len() as _);
            for child in children {
                let child = &nodes[*child];
                write_node(vec, nodes, None, child);
            }
        }
        NBTNode::Compound(value) => {
            if let Some(name) = name {
                vec.push(TAG_COMPOUND_ID.0);
                write_string(vec, name);
            }
            write_compound(vec, nodes, value);
        }
        NBTNode::IntArray(values) => {
            if let Some(name) = name {
                vec.push(TAG_INT_ARRAY_ID.0);
                write_string(vec, name);
            }
            vec.put_i32(values.len() as _);
            for value in values {
                vec.put_i32(*value);
            }
        }
        NBTNode::LongArray(values) => {
            if let Some(name) = name {
                vec.push(TAG_LONG_ARRAY_ID.0);
                write_string(vec, name);
            }
            vec.put_i32(values.len() as _);
            for value in values {
                vec.put_i64(*value);
            }
        }
    }
}

fn write_compound(vec: &mut Vec<u8>, nodes: &Vec<NBTNode>, children: &NBTCompound) {
    for (child_name, child_idx) in &children.0 {
        let child = &nodes[*child_idx];
        write_node(vec, nodes, Some(child_name), child);
    }

    vec.push(TAG_END_ID.0);
}

fn write_string(vec: &mut Vec<u8>, value: &str) {
    vec.put_u16(value.len() as _);
    vec.extend_from_slice(value.as_bytes());
}
