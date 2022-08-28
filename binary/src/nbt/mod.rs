use std::{fmt::Debug, result};

mod cached_nbt;
pub mod decode;
pub mod encode;
mod pretty;
pub mod stringified;

pub use cached_nbt::CachedNBT;

const TAG_END_ID: u8 = 0;
const TAG_BYTE_ID: u8 = 1;
const TAG_SHORT_ID: u8 = 2;
const TAG_INT_ID: u8 = 3;
const TAG_LONG_ID: u8 = 4;
const TAG_FLOAT_ID: u8 = 5;
const TAG_DOUBLE_ID: u8 = 6;
const TAG_BYTE_ARRAY_ID: u8 = 7;
const TAG_STRING_ID: u8 = 8;
const TAG_LIST_ID: u8 = 9;
const TAG_COMPOUND_ID: u8 = 10;
const TAG_INT_ARRAY_ID: u8 = 11;
const TAG_LONG_ARRAY_ID: u8 = 12;

#[derive(Clone)]
pub struct NBT {
    pub root_name: String,
    root_children: NBTCompound,
    nodes: Vec<NBTNode>,
}

impl Debug for NBT {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            pretty::to_pretty_debug(f, self)
        } else {
            stringified::to_snbt(f, self)
        }
    }
}

impl NBT {
    pub fn new() -> NBT {
        Self::new_named(String::new())
    }

    pub fn new_named(root_name: String) -> NBT {
        NBT {
            root_name,
            root_children: NBTCompound(Vec::new()),
            nodes: Vec::new(),
        }
    }

    pub fn find_root(&self, key: &str) -> Option<&NBTNode> {
        let idx = self.root_children.find(key)?;
        Some(&self.nodes[idx])
    }

    pub fn insert_root(&mut self, key: &str, value: NBTNode) {
        let idx = self.nodes.len();
        self.nodes.push(value);
        self.root_children.insert(key, idx);
    }

    pub fn find(&self, node: &NBTNode, key: &str) -> Option<&NBTNode> {
        match node {
            NBTNode::Compound(compound) => {
                let index = compound.find(key)?;
                Some(&self.nodes[index])
            }
            _ => None,
        }
    }

    pub fn iter<'a>(&'a self, node: &'a NBTNode) -> Option<NBTIterator<'a>> {
        match node {
            NBTNode::List {
                type_id: _,
                children,
            } => Some(NBTIterator {
                nbt: self,
                indices: children,
                index: 0,
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum NBTNode {
    // 32 bytes
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List { type_id: u8, children: Vec<usize> },
    Compound(NBTCompound),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl NBTNode {
    pub fn as_byte(&self) -> Option<i8> {
        match self {
            NBTNode::Byte(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_short(&self) -> Option<i16> {
        match self {
            NBTNode::Short(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i32> {
        match self {
            NBTNode::Int(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_long(&self) -> Option<i64> {
        match self {
            NBTNode::Long(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            NBTNode::Float(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_double(&self) -> Option<f64> {
        match self {
            NBTNode::Double(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_byte_array(&self) -> Option<&Vec<i8>> {
        match self {
            NBTNode::ByteArray(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&String> {
        match self {
            NBTNode::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_int_array(&self) -> Option<&Vec<i32>> {
        match self {
            NBTNode::IntArray(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_long_array(&self) -> Option<&Vec<i64>> {
        match self {
            NBTNode::LongArray(value) => Some(value),
            _ => None,
        }
    }
}

pub struct NBTIterator<'a> {
    nbt: &'a NBT,
    indices: &'a [usize],
    index: usize,
}

impl<'a> Iterator for NBTIterator<'a> {
    type Item = &'a NBTNode;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.indices.len() {
            None
        } else {
            let next = &self.nbt.nodes[self.indices[self.index]];
            self.index += 1;
            Some(next)
        }
    }
}

// Note: Using SmartString instead of String results in worse perf
#[derive(Debug, Clone)]
pub struct NBTCompound(Vec<(String, usize)>);

impl NBTCompound {
    fn find(&self, key: &str) -> Option<usize> {
        /*if self.0.len() < 8 {
            for (name, idx) in &self.0 {
                if name.as_str() == key {
                    return Some(*idx);
                }
            }
            return None;
        }*/

        match self.binary_search(key) {
            Ok(index) => Some(self.0[index].1),
            Err(_) => None,
        }
    }

    fn insert(&mut self, key: &str, value: usize) {
        match self.binary_search(key) {
            Ok(index) => {
                let _ = std::mem::replace(&mut self.0[index].1, value);
            }
            Err(index) => {
                self.0.insert(index, (key.into(), value));
            }
        }
    }

    fn binary_search(&self, key: &str) -> result::Result<usize, usize> {
        self.0.binary_search_by_key(&key, |v| v.0.as_str())
    }
}

#[test]
fn read_test() {
    // https://wiki.vg/NBT#bigtest.nbt

    let input = include_bytes!("../../../assets/bigtest.nbt");
    let nbt = decode::read(&mut input.as_slice()).unwrap();

    assert_eq!(nbt.root_name.as_str(), "Level");
    verify_bigtest_nbt(&nbt);
}

#[test]
fn read_and_write_test() {
    // https://wiki.vg/NBT#bigtest.nbt

    let input = include_bytes!("../../../assets/bigtest.nbt");
    let nbt = decode::read(&mut input.as_slice()).unwrap();
    let input = encode::write(&nbt);
    let nbt = decode::read(&mut input.as_slice()).unwrap();

    assert_eq!(nbt.root_name.as_str(), "Level");
    verify_bigtest_nbt(&nbt);
}

#[test]
fn to_from_snbt_test() {
    // https://wiki.vg/NBT#bigtest.nbt

    let input = include_bytes!("../../../assets/bigtest.nbt");
    let nbt = decode::read(&mut input.as_slice()).unwrap();
    let snbt = stringified::to_snbt_string(&nbt);
    let nbt = stringified::from_snbt(&snbt).unwrap();

    verify_bigtest_nbt(&nbt);
}

#[cfg(test)]
fn verify_bigtest_nbt(nbt: &NBT) {
    {
        // TAG_Compound('Level'): 11 entries
        {
            // TAG_Compound('nested compound test'): 2 entries
            let nested = nbt.find_root("nested compound test").unwrap();

            {
                // TAG_Compound('egg'): 2 entries
                let egg = nbt.find(nested, "egg").unwrap();

                // TAG_String('name'): 'Eggbert'
                let name = nbt.find(egg, "name").unwrap();
                assert_eq!(name.as_string(), Some(&"Eggbert".into()));

                // TAG_Float('value'): 0.5
                let value = nbt.find(egg, "value").unwrap();
                assert_eq!(value.as_float(), Some(0.5))
            }

            {
                // TAG_Compound('ham'): 2 entries
                let ham = nbt.find(nested, "ham").unwrap();

                // TAG_String('name'): 'Hampus'
                let name = nbt.find(ham, "name").unwrap();
                assert_eq!(name.as_string(), Some(&"Hampus".into()));

                // TAG_Float('value'): 0.75
                let value = nbt.find(ham, "value").unwrap();
                assert_eq!(value.as_float(), Some(0.75))
            }
        }

        // TAG_Int('intTest'): 2147483647
        let int_test = nbt.find_root("intTest").unwrap();
        assert_eq!(int_test.as_int(), Some(2147483647));

        // TAG_Byte('byteTest'): 127
        let byte_test = nbt.find_root("byteTest").unwrap();
        assert_eq!(byte_test.as_byte(), Some(127));

        // TAG_String('stringTest'): 'HELLO WORLD THIS IS A TEST STRING \xc5\xc4\xd6!'
        let string_test = nbt.find_root("stringTest").unwrap();
        assert_eq!(
            string_test.as_string(),
            Some(&"HELLO WORLD THIS IS A TEST STRING \u{c5}\u{c4}\u{d6}!".into())
        );

        // TAG_List('listTest (long)'): 5 entries
        let list_test = nbt.find_root("listTest (long)").unwrap();
        let mut list_test_iter = nbt.iter(list_test).unwrap();
        assert_eq!(list_test_iter.next().unwrap().as_long(), Some(11));
        assert_eq!(list_test_iter.next().unwrap().as_long(), Some(12));
        assert_eq!(list_test_iter.next().unwrap().as_long(), Some(13));
        assert_eq!(list_test_iter.next().unwrap().as_long(), Some(14));
        assert_eq!(list_test_iter.next().unwrap().as_long(), Some(15));
        assert!(list_test_iter.next().is_none());

        // TAG_Double('doubleTest'): 0.49312871321823148
        let double_test = nbt.find_root("doubleTest").unwrap();
        assert_eq!(double_test.as_double(), Some(0.49312871321823148));

        // TAG_Float('floatTest'): 0.49823147058486938
        let float_test = nbt.find_root("floatTest").unwrap();
        assert_eq!(float_test.as_float(), Some(0.49823147058486938));

        // TAG_Long('longTest'): 9223372036854775807L
        let long_test = nbt.find_root("longTest").unwrap();
        assert_eq!(long_test.as_long(), Some(9223372036854775807));

        // TAG_Short('shortTest'): 32767
        let short_test = nbt.find_root("shortTest").unwrap();
        assert_eq!(short_test.as_short(), Some(32767));

        // TAG_List('listTest (compound)'): 5 entries
        let list_test = nbt.find_root("listTest (compound)").unwrap();
        let mut list_test_iter = nbt.iter(list_test).unwrap();
        {
            // TAG_Compound(None): 2 entries
            let first = list_test_iter.next().unwrap();

            // TAG_Long('created-on'): 1264099775885L
            let created_on = nbt.find(first, "created-on").unwrap();
            assert_eq!(created_on.as_long(), Some(1264099775885));

            // TAG_String('name'): 'Compound tag #0'
            let name = nbt.find(first, "name").unwrap();
            assert_eq!(name.as_string(), Some(&"Compound tag #0".into()));
        }
        {
            // TAG_Compound(None): 2 entries
            let second = list_test_iter.next().unwrap();

            // TAG_Long('created-on'): 1264099775885L
            let created_on = nbt.find(second, "created-on").unwrap();
            assert_eq!(created_on.as_long(), Some(1264099775885));

            // TAG_String('name'): 'Compound tag #1'
            let name = nbt.find(second, "name").unwrap();
            assert_eq!(name.as_string(), Some(&"Compound tag #1".into()));
        }
        assert!(list_test_iter.next().is_none());

        // TAG_Byte_Array('byteArrayTest (the first 1000 values of (n*n*255+n*7)%100, starting with n=0 (0, 62, 34, 16, 8, ...))'): [1000 bytes]
        let byte_array_test = nbt.find_root("byteArrayTest (the first 1000 values of (n*n*255+n*7)%100, starting with n=0 (0, 62, 34, 16, 8, ...))").unwrap();
        let bytes: &[i8] = byte_array_test.as_byte_array().unwrap();
        assert_eq!(bytes.len(), 1000);
        for (index, value) in bytes.iter().enumerate() {
            let expected = (index * index * 255 + index * 7) % 100;
            assert_eq!(*value, expected as i8);
        }
    }
}
