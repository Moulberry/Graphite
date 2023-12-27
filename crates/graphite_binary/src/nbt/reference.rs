use std::hint::unreachable_unchecked;

use super::{NBTNode, NBT, TagType};

#[derive(Copy, Clone, Debug)]
pub enum NBTRef<'a> {
    Byte(&'a i8),
    Short(&'a i16),
    Int(&'a i32),
    Long(&'a i64),
    Float(&'a f32),
    Double(&'a f64),
    ByteArray(&'a Vec<i8>),
    String(&'a String),
    List(ListRef<'a>),
    Compound(CompoundRef<'a>),
    IntArray(&'a Vec<i32>),
    LongArray(&'a Vec<i64>),
}

macro_rules! as_basic {
    ($name:ident, $value_type:ty, $node:ident) => {
        paste::paste! {
            pub fn [<as_ $name>](self) -> Option<&'a $value_type> {
                match self {
                    NBTRef::$node(value) => Some(value),
                    _ => None,
                }
            }
        }
    }
}

impl <'a> NBTRef<'a> {
    super::enumerate_basic_types!(as_basic);

    pub fn as_compound(self) -> Option<CompoundRef<'a>> {
        match self {
            NBTRef::Compound(compound) => Some(compound),
            _ => None,
        }
    }

    pub fn as_list(self) -> Option<ListRef<'a>> {
        match self {
            NBTRef::List(list) => Some(list),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum NBTRefMut<'a> {
    Byte(&'a mut i8),
    Short(&'a mut i16),
    Int(&'a mut i32),
    Long(&'a mut i64),
    Float(&'a mut f32),
    Double(&'a mut f64),
    ByteArray(&'a mut Vec<i8>),
    String(&'a mut String),
    List(ListRefMut<'a>),
    Compound(CompoundRefMut<'a>),
    IntArray(&'a mut Vec<i32>),
    LongArray(&'a mut Vec<i64>),
}

macro_rules! as_basic_mut {
    ($name:ident, $value_type:ty, $node:ident) => {
        paste::paste! {
            pub fn [<as_ $name>](&mut self) -> Option<&mut $value_type> {
                match self {
                    NBTRefMut::$node(value) => Some(value),
                    _ => None,
                }
            }
        }
    }
}

impl <'a> NBTRefMut<'a> {
    super::enumerate_basic_types!(as_basic_mut);

    pub fn as_compound(self) -> Option<CompoundRefMut<'a>> {
        match self {
            NBTRefMut::Compound(compound) => Some(compound),
            _ => None,
        }
    }

    pub fn as_list(self) -> Option<ListRefMut<'a>> {
        match self {
            NBTRefMut::List(list) => Some(list),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct CompoundRef<'a> {
    pub(crate) nbt: &'a NBT,
    pub(crate) node_idx: usize
}

impl <'a> CompoundRef<'a> {
    fn find_idx(&self, key: &str) -> Option<usize> {
        match self.nbt.nodes.get(self.node_idx) {
            Some(NBTNode::Compound(compound)) => {
                compound.find(key)
            },
            _ => unsafe { unreachable_unchecked() }
        }
    }

    fn get_node(&self, idx: usize) -> &NBTNode {
        &self.nbt.nodes[idx]
    }

    super::enumerate_basic_types!(super::find);

    pub fn find_list(&self, key: &str, type_id: TagType) -> Option<ListRef<'_>> {
        let idx = self.find_idx(key)?;
        match self.get_node(idx) {
            NBTNode::List { type_id: list_type_id, children: _ } if *list_type_id == type_id => {
                Some(ListRef {
                    nbt: self.nbt,
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
                    nbt: self.nbt,
                    node_idx: idx
                })
            },
            _ => None
        }
    }

    pub fn find(&self, key: &str) -> Option<NBTRef<'_>> {
        let idx = self.find_idx(key)?;
        Some(self.nbt.get_reference(idx))
    }
}

#[derive(Debug)]
pub struct CompoundRefMut<'a> {
    pub(crate) nbt: &'a mut NBT,
    pub(crate) node_idx: usize
}

impl <'a> CompoundRefMut<'a> {
    fn insert_node(&mut self, key: &str, node: NBTNode) -> usize {
        let idx = self.nbt.nodes.len();
        self.nbt.nodes.push(node);

        match self.nbt.nodes.get_mut(self.node_idx) {
            Some(NBTNode::Compound(compound)) => {
                compound.insert(key, idx);
            },
            _ => unsafe { unreachable_unchecked() }
        };

        idx
    }

    fn find_idx(&self, key: &str) -> Option<usize> {
        match self.nbt.nodes.get(self.node_idx) {
            Some(NBTNode::Compound(compound)) => {
                compound.find(key)
            },
            _ => unsafe { unreachable_unchecked() }
        }
    }

    fn get_node(&self, idx: usize) -> &NBTNode {
        &self.nbt.nodes[idx]
    }

    fn get_node_mut(&mut self, idx: usize) -> &mut NBTNode {
        &mut self.nbt.nodes[idx]
    }

    super::enumerate_basic_types!(super::insert);
    super::enumerate_basic_types!(super::find);
    super::enumerate_basic_types!(super::find_mut);

    pub fn create_list(&mut self, key: &str, type_id: TagType) -> ListRefMut<'_> {
        let idx = self.insert_node(key, NBTNode::List { type_id, children: Default::default() });

        ListRefMut {
            nbt: self.nbt,
            node_idx: idx
        }
    }

    pub fn create_compound(&mut self, key: &str) -> CompoundRefMut<'_> {
        let idx = self.insert_node(key, NBTNode::Compound(Default::default()));

        CompoundRefMut {
            nbt: self.nbt,
            node_idx: idx
        }
    }

    pub fn find_list(&self, key: &str, type_id: TagType) -> Option<ListRef<'_>> {
        let idx = self.find_idx(key)?;
        match self.get_node(idx) {
            NBTNode::List { type_id: list_type_id, children: _ } if *list_type_id == type_id => {
                Some(ListRef {
                    nbt: self.nbt,
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
                    nbt: self.nbt,
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
                    nbt: self.nbt,
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
                    nbt: self.nbt,
                    node_idx: idx
                })
            },
            _ => None
        }
    }

    pub fn find(&self, key: &str) -> Option<NBTRef<'_>> {
        let idx = self.find_idx(key)?;
        Some(self.nbt.get_reference(idx))
    }

    pub fn find_mut(&mut self, key: &str) -> Option<NBTRefMut<'_>> {
        let idx = self.find_idx(key)?;
        Some(self.nbt.get_mutable_reference(idx))
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ListRef<'a> {
    pub(crate) nbt: &'a NBT,
    pub(crate) node_idx: usize
}

impl <'a> ListRef<'a> {
    pub fn iter(&self) -> NBTIterator<'_> {
        match self.nbt.nodes.get(self.node_idx) {
            Some(NBTNode::List { type_id: _, children} ) => {
                NBTIterator {
                    nbt: self.nbt,
                    indices: children,
                    index: 0,
                }
            },
            _ => unsafe { unreachable_unchecked() }
        }        
    }
}

#[derive(Debug)]
pub struct ListRefMut<'a> {
    pub(crate) nbt: &'a mut NBT,
    pub(crate) node_idx: usize
}

impl <'a> ListRefMut<'a> {
    fn insert_node(&mut self, node: NBTNode) -> usize {
        let idx = self.nbt.nodes.len();

        match self.nbt.nodes.get_mut(self.node_idx) {
            Some(NBTNode::List{type_id, children}) => {
                if *type_id != node.get_type() {
                    panic!("Tried to insert {:?} into a list of {:?}", node.get_type(), type_id);
                }
                children.push(idx);
            },
            _ => unsafe { unreachable_unchecked() }
        }

        self.nbt.nodes.push(node);
        idx
    }

    pub fn insert_byte(&mut self, value: i8) {
        self.insert_node(NBTNode::Byte(value));
    }

    pub fn create_compound(&mut self) -> CompoundRefMut<'_> {
        let idx = self.insert_node(NBTNode::Compound(Default::default()));

        CompoundRefMut {
            nbt: self.nbt,
            node_idx: idx
        }
    }
}

pub struct NBTIterator<'a> {
    nbt: &'a NBT,
    indices: &'a [usize],
    index: usize,
}

impl<'a> Iterator for NBTIterator<'a> {
    type Item = NBTRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.indices.len() {
            None
        } else {
            let next = self.nbt.get_reference(self.indices[self.index]);
            self.index += 1;
            Some(next)
        }
    }
}