
use anyhow::bail;
use enumset::EnumSet;
use graphite_binary::slice_serialization::*;
use graphite_mc_constants::{builtin::DataComponentType, item::Item};

use super::ItemStack;

#[derive(Clone, Debug)]
pub struct HashedStack {
    pub item: Item,
    pub count: i32,
    pub added_components: Vec<(DataComponentType, i32)>,
    pub removed_components: EnumSet<DataComponentType>
}

impl HashedStack {
    pub fn hashed_equals(&self, item_stack: &ItemStack) -> bool {
        if self.item == Item::Air || self.count <= 0 {
            return item_stack.is_empty();
        }
        if self.item != item_stack.item || self.count != item_stack.count {
            return false;
        }
        return item_stack.components.raw_hashed_equals(self.item, &self.added_components, self.removed_components);
    }
}

impl <'r,'a:'r>graphite_binary::slice_serialization::SliceSerializable<'r,'a>for HashedStack {
    type CopyType =  &'r HashedStack;

    fn read(bytes: &mut &'a[u8]) -> anyhow::Result<HashedStack>{
        let item = <VarInt as SliceSerializable<u16>>::read(bytes)?;
        let item: Item = item.try_into()?;
        let count = <VarInt as SliceSerializable<i32>>::read(bytes)?;

        let added_count: i32 = VarInt::read(bytes)?;
        if added_count < 0 {
            bail!("added count must not be negative, got {}", added_count);
        }
        if added_count > DataComponentType::COUNT as i32 {
            bail!("too many added data components, maximum is {}, got {}", DataComponentType::COUNT, added_count)
        }

        let mut added_components = Vec::with_capacity(added_count as usize);

        for _ in 0..added_count {
            let component_type: u8 = VarInt::read(bytes)?;
            let component_type: DataComponentType = component_type.try_into()?;
            
            let result = added_components.binary_search_by_key(&(component_type as u8), |(t, _)| *t as u8);
            match result {
                Ok(_) => bail!("duplicate component type: {:?}", component_type),
                Err(absent) => {
                    let value = BigEndian::read(bytes)?;
                    added_components.insert(absent, (component_type, value));
                },
            }
        }

        let removed_count: i32 = VarInt::read(bytes)?;
        if removed_count < 0 {
            bail!("removed count must not be negative, got {}", added_count);
        }
        if removed_count > DataComponentType::COUNT as i32 {
            bail!("too many removed data components, maximum is {}, got {}", DataComponentType::COUNT, added_count)
        }
        let mut removed_components = EnumSet::empty();
        for _ in 0..removed_count {
            let component_type: u8 = VarInt::read(bytes)?;
            let component_type: DataComponentType = component_type.try_into()?;

            removed_components |= component_type;
        }

        Ok(Self {
            item,
            count,
            added_components,
            removed_components,
        })
    }

    fn get_write_size(object: &'r HashedStack) -> usize {
        unimplemented!()
    }

    unsafe fn write<'bytes>(mut bytes: &'bytes mut [u8], object: &'r HashedStack) ->  &'bytes mut [u8]{
        unimplemented!()
    }

    #[inline(always)]
    fn as_copy_type(t: &'r HashedStack) -> Self::CopyType {
        t
    }

}