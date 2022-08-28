use std::borrow::Cow;

use super::*;
use crate::slice_serialization::{BigEndian, Single, SliceSerializable};
use anyhow::bail;

pub fn read(bytes: &mut &[u8]) -> anyhow::Result<NBT> {
    let type_id: u8 = Single::read(bytes)?;
    if type_id == TAG_END_ID {
        return Ok(NBT::new());
    } else if type_id != TAG_COMPOUND_ID {
        bail!("nbt_decode: root must be a compound");
    }

    let mut nodes = Vec::new();
    let name = read_string(bytes)?;
    let children = read_compound(bytes, &mut nodes)?;

    Ok(NBT {
        root_name: name.into_owned(),
        root_children: children,
        nodes,
    })
}

fn read_node(bytes: &mut &[u8], nodes: &mut Vec<NBTNode>, type_id: u8) -> anyhow::Result<usize> {
    debug_assert!(
        type_id != TAG_END_ID,
        "read_node must not be called with TAG_END"
    );

    let node = match type_id {
        TAG_BYTE_ID => NBTNode::Byte(Single::read(bytes)?),
        TAG_SHORT_ID => NBTNode::Short(BigEndian::read(bytes)?),
        TAG_INT_ID => NBTNode::Int(BigEndian::read(bytes)?),
        TAG_LONG_ID => NBTNode::Long(BigEndian::read(bytes)?),
        TAG_FLOAT_ID => NBTNode::Float(BigEndian::read(bytes)?),
        TAG_DOUBLE_ID => NBTNode::Double(BigEndian::read(bytes)?),
        TAG_BYTE_ARRAY_ID => NBTNode::ByteArray(read_byte_array(bytes)?),
        TAG_STRING_ID => NBTNode::String(read_string(bytes)?.into_owned()),
        TAG_LIST_ID => {
            let (type_id, children) = read_list(bytes, nodes)?;
            NBTNode::List { type_id, children }
        }
        TAG_COMPOUND_ID => NBTNode::Compound(read_compound(bytes, nodes)?),
        TAG_INT_ARRAY_ID => NBTNode::IntArray(read_int_array(bytes)?),
        TAG_LONG_ARRAY_ID => NBTNode::LongArray(read_long_array(bytes)?),
        _ => bail!("unknown type id: {}", type_id),
    };
    nodes.push(node);
    Ok(nodes.len() - 1)
}

fn read_compound(bytes: &mut &[u8], nodes: &mut Vec<NBTNode>) -> anyhow::Result<NBTCompound> {
    let mut children = NBTCompound(Vec::new());

    loop {
        let type_id: u8 = Single::read(bytes)?;
        if type_id == TAG_END_ID {
            break Ok(children);
        } else {
            let name = read_string(bytes)?;
            let node = read_node(bytes, nodes, type_id)?;

            match children.binary_search(name.as_ref()) {
                Ok(_) => bail!("read_compound: duplicate key"),
                Err(index) => {
                    children.0.insert(index, (name.into(), node));
                }
            }
        }
    }
}

fn read_byte_array(bytes: &mut &[u8]) -> anyhow::Result<Vec<i8>> {
    let length: i32 = BigEndian::read(bytes)?;
    if length < 0 {
        bail!("read_byte_array: length cannot be negative");
    } else if bytes.len() < length as _ {
        bail!("read_byte_array: not enough bytes to read byte array");
    }

    let (arr_bytes, rest_bytes) = bytes.split_at(length as _);
    *bytes = rest_bytes;

    let vec: Vec<u8> = arr_bytes.into();
    Ok(unsafe { std::mem::transmute(vec) })
}

fn read_string<'a>(bytes: &mut &'a [u8]) -> anyhow::Result<Cow<'a, str>> {
    let length: u16 = BigEndian::read(bytes)?;
    if bytes.len() < length as _ {
        bail!("read_string: not enough bytes to read string");
    }

    let (str_bytes, rest_bytes) = bytes.split_at(length as _);
    *bytes = rest_bytes;

    Ok(cesu8::from_java_cesu8(str_bytes)?)
}

fn read_list(bytes: &mut &[u8], nodes: &mut Vec<NBTNode>) -> anyhow::Result<(u8, Vec<usize>)> {
    let type_id: u8 = Single::read(bytes)?;

    let length: i32 = BigEndian::read(bytes)?;

    if length <= 0 {
        Ok((type_id, Vec::new()))
    } else if type_id == TAG_END_ID {
        bail!("read_list: type cannot be TAG_END for non-zero length list");
    } else {
        let mut children = Vec::with_capacity(length as _);

        for _ in 0..length {
            children.push(read_node(bytes, nodes, type_id)?);
        }

        Ok((type_id, children))
    }
}

fn read_int_array(bytes: &mut &[u8]) -> anyhow::Result<Vec<i32>> {
    let length: i32 = BigEndian::read(bytes)?;
    if length < 0 {
        bail!("read_int_array: length cannot be negative");
    } else if bytes.len() < (length as usize) * 4 {
        bail!("read_int_array: not enough bytes to read int array");
    }

    let mut values = Vec::with_capacity(length as _);
    for _ in 0..length {
        values.push(BigEndian::read(bytes)?);
    }
    Ok(values)
}

fn read_long_array(bytes: &mut &[u8]) -> anyhow::Result<Vec<i64>> {
    let length: i32 = BigEndian::read(bytes)?;
    if length < 0 {
        bail!("read_long_array: length cannot be negative");
    } else if bytes.len() < (length as usize) * 8 {
        bail!("read_long_array: not enough bytes to read long array");
    }

    let mut values = Vec::with_capacity(length as _);
    for _ in 0..length {
        values.push(BigEndian::read(bytes)?);
    }
    Ok(values)
}
