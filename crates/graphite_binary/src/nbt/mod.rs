use std::{fmt::Debug, result, ptr::NonNull};
use slab::Slab;
pub use reference::{NBTRef, NBTRefMut, ListRef, CompoundRef, ListRefMut, CompoundRefMut};
pub use encoded_nbt::EncodedNBT;

mod encoded_nbt;
pub mod decode;
pub mod encode;
pub mod encode_raw;
mod pretty;
pub mod stringified;

mod reference;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct TagType(pub(crate) u8);

pub const TAG_END_ID: TagType = TagType(0);
pub const TAG_BYTE_ID: TagType = TagType(1);
pub const TAG_SHORT_ID: TagType = TagType(2);
pub const TAG_INT_ID: TagType = TagType(3);
pub const TAG_LONG_ID: TagType = TagType(4);
pub const TAG_FLOAT_ID: TagType = TagType(5);
pub const TAG_DOUBLE_ID: TagType = TagType(6);
pub const TAG_BYTE_ARRAY_ID: TagType = TagType(7);
pub const TAG_STRING_ID: TagType = TagType(8);
pub const TAG_LIST_ID: TagType = TagType(9);
pub const TAG_COMPOUND_ID: TagType = TagType(10);
pub const TAG_INT_ARRAY_ID: TagType = TagType(11);
pub const TAG_LONG_ARRAY_ID: TagType = TagType(12);

#[derive(Clone)]
pub struct NBT {
    pub root_name: String,
    root_index: usize,
    nodes: Slab<NBTNode>,
}

impl Default for NBT {
    fn default() -> Self {
        Self::new()
    }
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

impl PartialEq for NBT {
    fn eq(&self, other: &Self) -> bool {
        self.as_reference() == other.as_reference()
    }
}

macro_rules! insert {
    ($name:ident, $value_type:ty, $node:ident) => {
        paste::paste! {
            pub fn [<insert_ $name>](&mut self, key: &str, value: $value_type) {
                self.insert_node(key, NBTNode::$node(value));
            }
        }
    }
}

macro_rules! get_list {
    ($name:ident, $value_type:ty, $node:ident) => {
        paste::paste! {
            pub fn [<get_ $name>](&self, index: usize) -> Option<&$value_type> {
                match self.get(index) {
                    Some(v) => v.[<as_ $name>](),
                    None => None,
                }
            }
        }
    }
}

macro_rules! insert_list {
    ($name:ident, $value_type:ty, $node:ident) => {
        paste::paste! {
            pub fn [<insert_ $name>](&mut self, value: $value_type) {
                self.insert_node(NBTNode::$node(value));
            }
        }
    }
}

macro_rules! set_list_at {
    ($name:ident, $value_type:ty, $node:ident) => {
        paste::paste! {
            pub fn [<set_ $name _at>](&mut self, index: usize, value: $value_type) {
                self.set_node_at(index, NBTNode::$node(value));
            }
        }
    }
}

macro_rules! find {
    ($name:ident, $value_type:ty, $node:ident) => {
        paste::paste! {
            pub fn [<find_ $name>](&self, key: &str) -> Option<&$value_type> {
                let idx = self.find_idx(key)?;
                match self.get_node(idx) {
                    NBTNode::$node(value) => Some(value),
                    _ => None
                }
            }
        }
    }
}

macro_rules! find_mut {
    ($name:ident, $value_type:ty, $node:ident) => {
        paste::paste! {
            pub fn [<find_ $name _mut>](&mut self, key: &str) -> Option<&mut $value_type> {
                let idx = self.find_idx(key)?;
                match self.get_node_mut(idx) {
                    NBTNode::$node(value) => Some(value),
                    _ => None
                }
            }
        }
    }
}

macro_rules! enumerate_basic_types {
    ($macro:path) => {
        $macro!(byte, i8, Byte);
        $macro!(short, i16, Short);
        $macro!(int, i32, Int);
        $macro!(long, i64, Long);
        $macro!(float, f32, Float);
        $macro!(double, f64, Double);
        $macro!(byte_array, Vec<i8>, ByteArray);
        $macro!(string, String, String);
        $macro!(int_array, Vec<i32>, IntArray);
        $macro!(long_array, Vec<i64>, LongArray);
    }
}

pub(crate) use enumerate_basic_types;
pub(crate) use insert;
pub(crate) use get_list;
pub(crate) use insert_list;
pub(crate) use set_list_at;
pub(crate) use find;
pub(crate) use find_mut;

impl NBT {
    pub fn new() -> NBT {
        Self::new_named(String::new())
    }

    pub fn new_named(root_name: String) -> NBT {
        let mut nodes = Slab::new();
        let root_index = nodes.insert(NBTNode::Compound(NBTCompound(Vec::new())));
        NBT {
            root_name,
            root_index,
            nodes,
        }
    }

    pub fn as_compound(&self) -> Option<CompoundRef<'_>> {
        match &self.nodes[self.root_index] {
            NBTNode::Compound(_) => {
                Some(CompoundRef { nbt: self, node_idx: self.root_index })
            },
            _ => None
        }
    }

    pub fn as_compound_mut(&mut self) -> Option<CompoundRefMut<'_>> {
        match &self.nodes[self.root_index] {
            NBTNode::Compound(_) => {
                let node_idx = self.root_index;
                Some(CompoundRefMut { nbt: self, node_idx })
            },
            _ => None
        }
    }

    pub fn as_reference(&self) -> NBTRef<'_> {
        self.get_reference(self.root_index)
    }

    pub fn as_reference_mut(&mut self) -> NBTRefMut<'_> {
        self.get_reference_mut(self.root_index)
    }

    fn remove_node(&mut self, idx: usize) {
        if idx == 0 {
            panic!("Cannot remove root node");
        }
        match self.nodes.remove(idx) {
            NBTNode::List { type_id: _, children } => {
                for child in children {
                    self.remove_node(child);
                }
            },
            NBTNode::Compound(compound) => {
                for (_, child) in compound.0 {
                    self.remove_node(child);
                }
            },
            _ => {}
        }
    }

    fn get_reference(&self, node_idx: usize) -> NBTRef<'_> {
        match &self.nodes[node_idx] {
            NBTNode::Byte(value) => NBTRef::Byte(value),
            NBTNode::Short(value) => NBTRef::Short(value),
            NBTNode::Int(value) => NBTRef::Int(value),
            NBTNode::Long(value) => NBTRef::Long(value),
            NBTNode::Float(value) => NBTRef::Float(value),
            NBTNode::Double(value) => NBTRef::Double(value),
            NBTNode::ByteArray(value) => NBTRef::ByteArray(value),
            NBTNode::String(value) => NBTRef::String(value),
            NBTNode::List { type_id, children: _ } => {
                NBTRef::List(ListRef { nbt: self, node_idx, children_type: *type_id })
            },
            NBTNode::Compound(_) => {
                NBTRef::Compound(CompoundRef { nbt: self, node_idx })
            },
            NBTNode::IntArray(value) => NBTRef::IntArray(value),
            NBTNode::LongArray(value) => NBTRef::LongArray(value),
        }
    }

    fn get_reference_mut(&mut self, node_idx: usize) -> NBTRefMut<'_> {
        // Ptr shenanigans because https://github.com/rust-lang/rust/issues/54663
        let mut nbt_ptr: NonNull<NBT> = self.into();

        match &mut self.nodes[node_idx] {
            NBTNode::Byte(value) => NBTRefMut::Byte(value),
            NBTNode::Short(value) => NBTRefMut::Short(value),
            NBTNode::Int(value) => NBTRefMut::Int(value),
            NBTNode::Long(value) => NBTRefMut::Long(value),
            NBTNode::Float(value) => NBTRefMut::Float(value),
            NBTNode::Double(value) => NBTRefMut::Double(value),
            NBTNode::ByteArray(value) => NBTRefMut::ByteArray(value),
            NBTNode::String(value) => NBTRefMut::String(value),
            NBTNode::List { type_id: _, children: _ } => {
                NBTRefMut::List(ListRefMut { nbt: unsafe { nbt_ptr.as_mut() }, node_idx })
            },
            NBTNode::Compound(_) => {
                NBTRefMut::Compound(CompoundRefMut { nbt: unsafe { nbt_ptr.as_mut() }, node_idx })
            },
            NBTNode::IntArray(value) => NBTRefMut::IntArray(value),
            NBTNode::LongArray(value) => NBTRefMut::LongArray(value),
        }
    }
}

#[derive(Debug, Clone)]
enum NBTNode {
    // 32 bytes
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List { type_id: TagType, children: Vec<usize> },
    Compound(NBTCompound),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl NBTNode {
    pub fn get_type(&self) -> TagType {
        match self {
            NBTNode::Byte(_) => TAG_BYTE_ID,
            NBTNode::Short(_) => TAG_SHORT_ID,
            NBTNode::Int(_) => TAG_INT_ID,
            NBTNode::Long(_) => TAG_LONG_ID,
            NBTNode::Float(_) => TAG_FLOAT_ID,
            NBTNode::Double(_) => TAG_DOUBLE_ID,
            NBTNode::ByteArray(_) => TAG_BYTE_ARRAY_ID,
            NBTNode::String(_) => TAG_STRING_ID,
            NBTNode::List { type_id: _, children: _ } => TAG_LIST_ID,
            NBTNode::Compound(_) => TAG_COMPOUND_ID,
            NBTNode::IntArray(_) => TAG_INT_ARRAY_ID,
            NBTNode::LongArray(_) => TAG_LONG_ARRAY_ID,
        }
    }
}

// Note: Using SmartString instead of String results in worse perf
#[derive(Debug, Clone, Default)]
pub(crate) struct NBTCompound(Vec<(String, usize)>);

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

    fn remove(&mut self, key: &str) -> Option<usize> {
        match self.binary_search(key) {
            Ok(index) => Some(self.0.remove(index).1),
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

    let input = include_bytes!("../../../../assets/bigtest.nbt");
    let nbt = decode::read_named(&mut input.as_slice()).unwrap();

    assert_eq!(nbt.root_name.as_str(), "Level");
    verify_bigtest_nbt(&nbt);
}

#[test]
fn read_and_write_test() {
    // https://wiki.vg/NBT#bigtest.nbt

    let input = include_bytes!("../../../../assets/bigtest.nbt");
    let nbt = decode::read_named(&mut input.as_slice()).unwrap();
    let input = encode::write_named(&nbt);
    let nbt = decode::read_named(&mut input.as_slice()).unwrap();
    
    assert_eq!(nbt.root_name.as_str(), "Level");
    verify_bigtest_nbt(&nbt);
}

#[test]
fn to_from_snbt_test() {
    // https://wiki.vg/NBT#bigtest.nbt

    let input = include_bytes!("../../../../assets/bigtest.nbt");
    let nbt = decode::read_named(&mut input.as_slice()).unwrap();
    let snbt = stringified::to_snbt_string(&nbt);
    let nbt = stringified::from_snbt(&snbt).unwrap();

    verify_bigtest_nbt(&nbt);
}

#[cfg(test)]
fn verify_bigtest_nbt(nbt: &NBT) {
    {
        let nbt = nbt.as_compound().unwrap();
        // TAG_Compound('Level'): 11 entries
        {
            // TAG_Compound('nested compound test'): 2 entries
            let nested = nbt.find_compound("nested compound test").unwrap();

            {
                // TAG_Compound('egg'): 2 entries
                let egg = nested.find_compound("egg").unwrap();

                // TAG_String('name'): 'Eggbert'
                let name = egg.find("name").unwrap();
                assert_eq!(name.as_string(), Some(&"Eggbert".into()));

                // TAG_Float('value'): 0.5
                let value = egg.find("value").unwrap();
                assert_eq!(value.as_float(), Some(&0.5))
            }

            {
                // TAG_Compound('ham'): 2 entries
                let ham = nested.find_compound("ham").unwrap();

                // TAG_String('name'): 'Hampus'
                let name = ham.find("name").unwrap();
                assert_eq!(name.as_string(), Some(&"Hampus".into()));

                // TAG_Float('value'): 0.75
                let value = ham.find("value").unwrap();
                assert_eq!(value.as_float(), Some(&0.75))
            }
        }

        // TAG_Int('intTest'): 2147483647
        let int_test = nbt.find("intTest").unwrap();
        assert_eq!(int_test.as_int(), Some(&2147483647));

        // TAG_Byte('byteTest'): 127
        let byte_test = nbt.find("byteTest").unwrap();
        assert_eq!(byte_test.as_byte(), Some(&127));

        // TAG_String('stringTest'): 'HELLO WORLD THIS IS A TEST STRING \xc5\xc4\xd6!'
        let string_test = nbt.find("stringTest").unwrap();
        assert_eq!(
            string_test.as_string(),
            Some(&"HELLO WORLD THIS IS A TEST STRING \u{c5}\u{c4}\u{d6}!".into())
        );

        // TAG_List('listTest (long)'): 5 entries
        let list_test = nbt.find_list("listTest (long)", TAG_LONG_ID).unwrap();
        let mut list_test_iter = list_test.iter();
        assert_eq!(list_test_iter.next().unwrap().as_long(), Some(&11));
        assert_eq!(list_test_iter.next().unwrap().as_long(), Some(&12));
        assert_eq!(list_test_iter.next().unwrap().as_long(), Some(&13));
        assert_eq!(list_test_iter.next().unwrap().as_long(), Some(&14));
        assert_eq!(list_test_iter.next().unwrap().as_long(), Some(&15));
        assert!(list_test_iter.next().is_none());

        // TAG_Double('doubleTest'): 0.49312871321823148
        let double_test = nbt.find("doubleTest").unwrap();
        assert_eq!(double_test.as_double(), Some(&0.49312871321823148));

        // TAG_Float('floatTest'): 0.49823147058486938
        let float_test = nbt.find("floatTest").unwrap();
        assert_eq!(float_test.as_float(), Some(&0.49823147058486938));

        // TAG_Long('longTest'): 9223372036854775807L
        let long_test = nbt.find("longTest").unwrap();
        assert_eq!(long_test.as_long(), Some(&9223372036854775807));

        // TAG_Short('shortTest'): 32767
        let short_test = nbt.find("shortTest").unwrap();
        assert_eq!(short_test.as_short(), Some(&32767));

        // TAG_List('listTest (compound)'): 5 entries
        let list_test = nbt.find_list("listTest (compound)", TAG_COMPOUND_ID).unwrap();
        let mut list_test_iter = list_test.iter();
        {
            // TAG_Compound(None): 2 entries
            let first = list_test_iter.next().unwrap().as_compound().unwrap();

            // TAG_Long('created-on'): 1264099775885L
            let created_on = first.find("created-on").unwrap();
            assert_eq!(created_on.as_long(), Some(&1264099775885));

            // TAG_String('name'): 'Compound tag #0'
            let name = first.find("name").unwrap();
            assert_eq!(name.as_string(), Some(&"Compound tag #0".into()));
        }
        {
            // TAG_Compound(None): 2 entries
            let second = list_test_iter.next().unwrap().as_compound().unwrap();

            // TAG_Long('created-on'): 1264099775885L
            let created_on = second.find("created-on").unwrap();
            assert_eq!(created_on.as_long(), Some(&1264099775885));

            // TAG_String('name'): 'Compound tag #1'
            let name = second.find("name").unwrap();
            assert_eq!(name.as_string(), Some(&"Compound tag #1".into()));
        }
        assert!(list_test_iter.next().is_none());

        // TAG_Byte_Array('byteArrayTest (the first 1000 values of (n*n*255+n*7)%100, starting with n=0 (0, 62, 34, 16, 8, ...))'): [1000 bytes]
        let byte_array = nbt.find_byte_array("byteArrayTest (the first 1000 values of (n*n*255+n*7)%100, starting with n=0 (0, 62, 34, 16, 8, ...))").unwrap();
        assert_eq!(byte_array.len(), 1000);
        for (index, value) in byte_array.iter().enumerate() {
            let expected = (index * index * 255 + index * 7) % 100;
            assert_eq!(*value, expected as i8);
        }
    }
}
