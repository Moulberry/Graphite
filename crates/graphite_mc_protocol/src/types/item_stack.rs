use std::cell::Cell;

use enumset::EnumSet;
use graphite_binary::{nbt::{CompoundRef, TAG_BYTE_ID, TAG_FLOAT_ID, TAG_INT_ID, TAG_STRING_ID}, slice_serialization::*};
use graphite_mc_constants::{builtin::DataComponentType, item::Item, types::EquipmentSlot};

use super::{data_component::{ArbitraryCustomModelData, CustomModelData, DataComponentMap, DataComponentTrait, Equippable, MaxStackSize, TooltipDisplay}, hash_ops::HashOps};

#[derive(PartialEq, Debug, Clone)]
pub struct ItemStack {
    pub item: Item,
    pub count: i32,
    pub components: DataComponentMap,
}

static EMPTY_STATIC: ItemStack = ItemStack {
    item: Item::Air,
    count: 0,
    components: DataComponentMap::new()
};

impl ItemStack {
    pub const EMPTY: Self = Self {
        item: Item::Air,
        count: 0,
        components: DataComponentMap::new()
    };

    pub fn empty_reference() -> &'static Self {
        &EMPTY_STATIC
    }

    pub const fn new(item: Item) -> Self {
        Self {
            item,
            count: 1,
            components: DataComponentMap::new()
        }
    }

    pub const fn with_count(mut self, count: i32) -> Self {
        self.count = count;
        self
    }

    pub fn load_from_nbt(compound: CompoundRef<'_>) -> Option<Self> {
        let item_string = compound.find_string("id")?;

        let item_u16 = graphite_mc_constants::item::string_to_u16(item_string).unwrap();
        let item_type: Item = item_u16.try_into().ok()?;

        let mut components = DataComponentMap::new();

        if let Some(tag) = compound.find_compound("tag") {
            if let Some(custom_model_data) = tag.find_numeric("CustomModelData") {
                components.set(CustomModelData::Float(custom_model_data));
            }
        }
        if let Some(tag) = compound.find_compound("components") {
            if let Some(custom_model_data) = tag.find_numeric("minecraft:custom_model_data") {
                components.set(CustomModelData::Float(custom_model_data));
            } else if let Some(custom_model_data) = tag.find_compound("minecraft:custom_model_data") {
                let mut floats = Vec::new();
                if let Some(floats_list) = custom_model_data.find_list("floats", TAG_FLOAT_ID) {
                    floats.reserve_exact(floats_list.len());
                    for tag in floats_list.iter() {
                        floats.push(tag.as_float().copied().unwrap());
                    }
                }

                let mut flags = Vec::new();
                if let Some(bytes_list) = custom_model_data.find_list("flags", TAG_BYTE_ID) {
                    flags.reserve_exact(bytes_list.len());
                    for tag in bytes_list.iter() {
                        flags.push(tag.as_byte().copied().unwrap() != 0);
                    }
                }

                let mut strings = Vec::new();
                if let Some(strings_list) = custom_model_data.find_list("strings", TAG_STRING_ID) {
                    strings.reserve_exact(strings_list.len());
                    for tag in strings_list.iter() {
                        strings.push(tag.as_string().cloned().unwrap());
                    }
                }

                let mut colours = Vec::new();
                if let Some(ints_list) = custom_model_data.find_list("colors", TAG_INT_ID) {
                    colours.reserve_exact(ints_list.len());
                    for tag in ints_list.iter() {
                        colours.push(tag.as_int().copied().unwrap());
                    }
                }

                components.set(CustomModelData::from_vecs(floats, flags, strings, colours));
            }
        }

        Some(ItemStack {
            item: item_type,
            count: 1,
            components,
        })
    }

    pub fn new_with_custom_model_data(item: Item, custom_model_data: i32) -> Self {
        let mut components = DataComponentMap::new();
        components.set(CustomModelData::Float(custom_model_data as f32));
        Self {
            item,
            count: 1,
            components
        }
    }

    pub const fn new_with_count(item: Item, count: i32) -> Self {
        Self {
            item,
            count,
            components: DataComponentMap::new()
        }
    }

    pub fn as_bytes(&self) -> Box<[u8]> {
        let write_size = ItemStack::get_write_size(self);
        let mut vec = Vec::with_capacity(write_size);

        unsafe {
            let ptr = vec.as_mut_ptr();
            let slice = std::slice::from_raw_parts_mut(ptr, write_size);
            let after = ItemStack::write(slice, self);
            let written = write_size - after.len();
            vec.set_len(written);
        };

        vec.into_boxed_slice()
    }

    pub fn set_hide_tooltip(&mut self, hide_tooltip: bool) {
        if let Some(tooltip_display) = self.components.get_mut::<TooltipDisplay>() {
            tooltip_display.hide_tooltip = hide_tooltip;
        } else if hide_tooltip {
            self.components.set(TooltipDisplay {
                hide_tooltip,
                hidden_components: EnumSet::empty(),
            });
        }
    }

    pub fn hide_component(&mut self, component: DataComponentType) {
        if let Some(tooltip_display) = self.components.get_mut::<TooltipDisplay>() {
            tooltip_display.hidden_components |= component;
        } else {
            self.components.set(TooltipDisplay {
                hide_tooltip: false,
                hidden_components: EnumSet::only(component),
            });
        }
    }

    pub fn set_max_stack_size(&mut self, size: usize) {
        if size == self.item.get_properties().max_stack_size as usize {
            self.components.remove::<MaxStackSize>();
        } else {
            self.components.set(MaxStackSize {
                size,
            });
        }
    }
    
    pub fn get_equipment_slot(&self) -> Option<EquipmentSlot> {
        if let Some(equippable) = self.components.get::<Equippable>() {
            Some(equippable.inner.slot)
        } else {
            None
        }
    }

    pub fn get<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&self) -> Option<&T> {
        self.components.get::<T>()
    }

    pub fn get_mut<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&mut self) -> Option<&mut T> {
        self.components.get_mut::<T>()
    }

    pub fn get_max_stack_size(&self) -> usize {
        if let Some(max_stack_size) = self.components.get::<MaxStackSize>() {
            max_stack_size.get()
        } else {
            self.item.get_properties().max_stack_size as usize
        }
    }

    pub fn is_empty(&self) -> bool {
        self.item == Item::Air || self.count <= 0
    }

    pub fn visual_count(&self) -> i32 {
        if self.item == Item::Air {
            0
        } else {
            self.count
        }
    }

    pub fn not_empty(self: Self) -> Option<Self> {
        if self.is_empty() {
            None
        } else {
            Some(self)
        }
    }

    pub fn equals_ignore_count(&self, other: &ItemStack) -> bool {
        self.item == other.item && self.components == other.components
    }

    pub fn checksum(&self) -> i32 {
        let mut map = HashOps::start_map();
        let id: &'static str = self.item.into();
        map.put_string("id", &format!("minecraft:{}", id));
        map.put_int("count", self.count);
        if !self.components.present().is_empty() {
            let checksum = self.components.checksum(self.item);
            map.put_raw_checksum("components", checksum);
        }
        map.finish()
    }
}

thread_local! {
    pub static ITEM_STACK_DECODE_DEPTH: Cell<u8> = Cell::new(0);
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd> for ItemStack {
    type CopyType = &'r ItemStack;

    fn as_copy_type(t: &'r Self) -> Self::CopyType {
        t
    }

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Self> {
        let count: i32 = VarInt::read(bytes)?;
        if count <= 0 {
            return Ok(Self::EMPTY);
        }

        let item: u16 = VarInt::read(bytes)?;
        let item: Item = item.try_into()?;

        let depth = ITEM_STACK_DECODE_DEPTH.get();
        if depth > 16 {
            anyhow::bail!("ItemStack decode too deep");
        }
        ITEM_STACK_DECODE_DEPTH.set(depth + 1);
        
        let components = match DataComponentMap::read(bytes) {
            Ok(v) => v,
            Err(e) => {
                ITEM_STACK_DECODE_DEPTH.set(depth);
                return Err(e);
            },
        };
        ITEM_STACK_DECODE_DEPTH.set(depth);

        Ok(ItemStack {
            item,
            count,
            components,
        })
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        if data.is_empty() {
            <VarInt as SliceSerializable<i32>>::write(bytes, 0)
        } else {
            bytes = <VarInt as SliceSerializable<i32>>::write(bytes, data.count);
            bytes = <VarInt as SliceSerializable<u16>>::write(bytes, data.item as u16);
            bytes = DataComponentMap::write(bytes, &data.components);
            bytes
        }

    }

    fn get_write_size(data: Self::CopyType) -> usize {
        if data.is_empty() {
            <VarInt as SliceSerializable<i32>>::get_write_size(0)
        } else {
            <VarInt as SliceSerializable<i32>>::get_write_size(data.count) +
                <VarInt as SliceSerializable<u16>>::get_write_size(data.item as u16) +
                DataComponentMap::get_write_size(&data.components)
        }
    }
}


impl<'a> Default for ItemStack {
    fn default() -> Self {
        Self::EMPTY
    }
}
