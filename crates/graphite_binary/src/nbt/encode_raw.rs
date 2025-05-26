use super::*;

use bytes::BufMut;

pub fn start_compound_tag(vec: &mut Vec<u8>, name: Option<&str>) {
    vec.push(TAG_COMPOUND_ID.0);

    if let Some(name) = name {
        write_string(vec, name);
    }
}

pub fn end_compound_tag(vec: &mut Vec<u8>) {
    vec.push(TAG_END_ID.0);
}

pub fn start_list(vec: &mut Vec<u8>, name: Option<&str>, type_id: TagType, children: i32) {
    if let Some(name) = name {
        vec.push(TAG_LIST_ID.0);
        write_string(vec, name);
    }

    vec.push(type_id.0);
    vec.put_i32(children);
}

pub fn push_byte(vec: &mut Vec<u8>, name: Option<&str>, value: i8) {
    if let Some(name) = name {
        vec.push(TAG_BYTE_ID.0);
        write_string(vec, name);
    }
    vec.put_i8(value);
}

pub fn push_short(vec: &mut Vec<u8>, name: Option<&str>, value: i16) {
    if let Some(name) = name {
        vec.push(TAG_SHORT_ID.0);
        write_string(vec, name);
    }
    vec.put_i16(value);
}

pub fn push_int(vec: &mut Vec<u8>, name: Option<&str>, value: i32) {
    if let Some(name) = name {
        vec.push(TAG_INT_ID.0);
        write_string(vec, name);
    }
    vec.put_i32(value);
}

pub fn push_long(vec: &mut Vec<u8>, name: Option<&str>, value: i64) {
    if let Some(name) = name {
        vec.push(TAG_LONG_ID.0);
        write_string(vec, name);
    }
    vec.put_i64(value);
}

pub fn push_float(vec: &mut Vec<u8>, name: Option<&str>, value: f32) {
    if let Some(name) = name {
        vec.push(TAG_FLOAT_ID.0);
        write_string(vec, name);
    }
    vec.put_f32(value);
}

pub fn push_double(vec: &mut Vec<u8>, name: Option<&str>, value: f64) {
    if let Some(name) = name {
        vec.push(TAG_DOUBLE_ID.0);
        write_string(vec, name);
    }
    vec.put_f64(value);
}

pub fn push_byte_array(vec: &mut Vec<u8>, name: Option<&str>, values: &[i8]) {
    if let Some(name) = name {
        vec.push(TAG_BYTE_ARRAY_ID.0);
        write_string(vec, name);
    }
    vec.put_i32(values.len() as _);
    vec.extend_from_slice(unsafe { std::mem::transmute(values) });
}

pub fn push_str(vec: &mut Vec<u8>, name: Option<&str>, value: &str) {
    if let Some(name) = name {
        vec.push(TAG_STRING_ID.0);
        write_string(vec, name);
    }
    write_string(vec, value);
}

fn write_string(vec: &mut Vec<u8>, value: &str) {
    vec.put_u16(value.len() as _);
    vec.extend_from_slice(value.as_bytes());
}
