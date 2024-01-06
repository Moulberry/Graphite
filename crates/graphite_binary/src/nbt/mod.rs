use std::{fmt::Debug, result, ptr::NonNull};

mod cached_nbt;
pub mod decode;
pub mod encode;
mod pretty;
pub mod stringified;

mod reference;
pub use reference::{NBTRef, NBTRefMut, ListRef, CompoundRef, ListRefMut, CompoundRefMut};

pub use cached_nbt::CachedNBT;

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

impl PartialEq for NBT {
    fn eq(&self, other: &Self) -> bool {
        if self.root_name != other.root_name {
            return false;
        }

        let self_children = &self.root_children.0;
        let other_children = &other.root_children.0;
        if self_children.len() != other_children.len() {
            return false;
        }

        let zipped = self_children.iter().zip(other_children.iter());
        for ((self_name, self_idx), (other_name, other_idx)) in zipped {
            if self_name != other_name {
                return false;
            }

            let self_element = self.get_reference(*self_idx);
            let other_element = other.get_reference(*other_idx);

            if self_element != other_element {
                return false;
            }
        }

        true
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
pub(crate) use find;
pub(crate) use find_mut;

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

    pub fn keys(&self) -> Vec<&str> {
        let mut refs: Vec<&str> = vec![];
        for (ele, _) in self.root_children.0.iter() {
            refs.push(ele);
        }
        refs
    }

    fn insert_node(&mut self, key: &str, node: NBTNode) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(node);
        self.root_children.insert(key, idx);

        // todo: delete previous node if this is replacing something

        idx
    }

    fn find_idx(&self, key: &str) -> Option<usize> {
        self.root_children.find(key)
    }

    fn get_node(&self, idx: usize) -> &NBTNode {
        &self.nodes[idx]
    }

    fn get_node_mut(&mut self, idx: usize) -> &mut NBTNode {
        &mut self.nodes[idx]
    }

    fn get_reference(&self, node_idx: usize) -> NBTRef<'_> {
        match self.get_node(node_idx) {
            NBTNode::Byte(value) => NBTRef::Byte(value),
            NBTNode::Short(value) => NBTRef::Short(value),
            NBTNode::Int(value) => NBTRef::Int(value),
            NBTNode::Long(value) => NBTRef::Long(value),
            NBTNode::Float(value) => NBTRef::Float(value),
            NBTNode::Double(value) => NBTRef::Double(value),
            NBTNode::ByteArray(value) => NBTRef::ByteArray(value),
            NBTNode::String(value) => NBTRef::String(value),
            NBTNode::List { type_id: _, children: _ } => {
                NBTRef::List(ListRef { nbt: self, node_idx })
            },
            NBTNode::Compound(_) => {
                NBTRef::Compound(CompoundRef { nbt: self, node_idx })
            },
            NBTNode::IntArray(value) => NBTRef::IntArray(value),
            NBTNode::LongArray(value) => NBTRef::LongArray(value),
        }
    }

    fn get_mutable_reference(&mut self, node_idx: usize) -> NBTRefMut<'_> {
        // Ptr shenanigans because https://github.com/rust-lang/rust/issues/54663
        let mut nbt_ptr: NonNull<NBT> = self.into();

        match self.get_node_mut(node_idx) {
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

    enumerate_basic_types!(insert);
    enumerate_basic_types!(find);
    enumerate_basic_types!(find_mut);

    pub fn create_list(&mut self, key: &str, type_id: TagType) -> ListRefMut<'_> {
        let idx = self.insert_node(key, NBTNode::List { type_id, children: Default::default() });

        ListRefMut {
            nbt: self,
            node_idx: idx
        }
    }

    pub fn create_compound(&mut self, key: &str) -> CompoundRefMut<'_> {
        let idx = self.insert_node(key, NBTNode::Compound(Default::default()));

        CompoundRefMut {
            nbt: self,
            node_idx: idx
        }
    }

    pub fn find_list(&self, key: &str, type_id: TagType) -> Option<ListRef<'_>> {
        let idx = self.find_idx(key)?;
        match self.get_node(idx) {
            NBTNode::List { type_id: list_type_id, children: _ } if *list_type_id == type_id => {
                Some(ListRef {
                    nbt: self,
                    node_idx: idx
                })
            },
            _ => None
        }
    }

    pub fn find_compound(&self, key: &str) -> Option<CompoundRef<'_>> {
        let idx = self.find_idx(key)?;
        match self.get_node(idx) {
            NBTNode::Compound(_) => {
                Some(CompoundRef {
                    nbt: self,
                    node_idx: idx
                })
            },
            _ => None
        }
    }

    pub fn find_list_mut(&mut self, key: &str, type_id: TagType) -> Option<ListRefMut<'_>> {
        let idx = self.find_idx(key)?;
        match self.get_node(idx) {
            NBTNode::List { type_id: list_type_id, children: _ } if *list_type_id == type_id => {
                Some(ListRefMut {
                    nbt: self,
                    node_idx: idx
                })
            },
            _ => None
        }
    }

    pub fn find_compound_mut(&mut self, key: &str) -> Option<CompoundRefMut<'_>> {
        let idx = self.find_idx(key)?;
        match self.get_node(idx) {
            NBTNode::Compound(_) => {
                Some(CompoundRefMut {
                    nbt: self,
                    node_idx: idx
                })
            },
            _ => None
        }
    }

    pub fn find(&self, key: &str) -> Option<NBTRef<'_>> {
        let idx = self.find_idx(key)?;
        Some(self.get_reference(idx))
    }

    pub fn find_mut(&mut self, key: &str) -> Option<NBTRefMut<'_>> {
        let idx = self.find_idx(key)?;
        Some(self.get_mutable_reference(idx))
    }

    // pub fn find_root(&self, key: &str) -> Option<&NBTNode> {
    //     let idx = self.root_children.find(key)?;
    //     Some(&self.nodes[idx])
    // }

    // pub fn insert_root(&mut self, key: &str, value: NBTNode) {
    //     let idx = self.nodes.len();
    //     self.nodes.push(value);
    //     self.root_children.insert(key, idx);
    // }

    // pub fn find(&self, node: &NBTNode, key: &str) -> Option<&NBTNode> {
    //     match node {
    //         NBTNode::Compound(compound) => {
    //             let index = compound.find(key)?;
    //             Some(&self.nodes[index])
    //         }
    //         _ => None,
    //     }
    // }

    // pub fn insert(&mut self, node: &mut NBTNode, key: &str, value: NBTNode) {
    //     match node {
    //         NBTNode::Compound(ref mut compound) => {
    //             let idx = self.nodes.len();
    //             self.nodes.push(value);
    //             compound.insert(key, idx);
    //         }
    //         _ => panic!("nbt insert: node is not a compound"),
    //     }
    // }

    // pub fn iter<'a>(&'a self, node: &'a NBTNode) -> Option<NBTIterator<'a>> {
    //     match node {
    //         NBTNode::List {
    //             type_id: _,
    //             children,
    //         } => Some(NBTIterator {
    //             nbt: self,
    //             indices: children,
    //             index: 0,
    //         }),
    //         _ => None,
    //     }
    // }

    // pub fn append(&mut self, node: &mut NBTNode, value: NBTNode) {
    //     match node {
    //         NBTNode::List {
    //             type_id,
    //             children,
    //         } => {
    //             if *type_id != value.get_type() {
    //                 panic!("nbt append: tag type is incorrect")
    //             }
    //             let idx = self.nodes.len();
    //             self.nodes.push(value);
    //             children.push(idx);
    //         },
    //         _ => panic!("nbt append: node is not a list"),
    //     }
    // }
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

// pub struct NBTIterator<'a> {
//     nbt: &'a NBT,
//     indices: &'a [usize],
//     index: usize,
// }

// impl<'a> Iterator for NBTIterator<'a> {
//     type Item = &'a NBTNode;

//     fn next(&mut self) -> Option<Self::Item> {
//         if self.index >= self.indices.len() {
//             None
//         } else {
//             let next = &self.nbt.nodes[self.indices[self.index]];
//             self.index += 1;
//             Some(next)
//         }
//     }
// }

// Note: Using SmartString instead of String results in worse perf
#[derive(Debug, Clone, Default)]
struct NBTCompound(Vec<(String, usize)>);

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

    let input = include_bytes!("../../../../assets/bigtest.nbt");
    let nbt = decode::read(&mut input.as_slice()).unwrap();

    assert_eq!(nbt.root_name.as_str(), "Level");
    verify_bigtest_nbt(&nbt);
}

#[test]
fn read_and_write_test() {
    // https://wiki.vg/NBT#bigtest.nbt

    let input = include_bytes!("../../../../assets/bigtest.nbt");
    let nbt = decode::read(&mut input.as_slice()).unwrap();
    let input = encode::write_named(&nbt);
    let nbt = decode::read(&mut input.as_slice()).unwrap();
    
    assert_eq!(nbt.root_name.as_str(), "Level");
    verify_bigtest_nbt(&nbt);
}

#[test]
fn to_from_snbt_test() {
    // https://wiki.vg/NBT#bigtest.nbt

    let input = include_bytes!("../../../../assets/bigtest.nbt");
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
