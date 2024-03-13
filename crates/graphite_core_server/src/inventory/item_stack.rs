use std::borrow::Cow;

use graphite_binary::nbt::CachedNBT;
use graphite_mc_constants::item::{Item, ItemProperties, NoSuchItemError};
use graphite_mc_protocol::types::ProtocolItemStack;

#[derive(Clone, Debug)]
pub struct ItemStack {
    pub item: Item,
    pub count: i8,
    properties: &'static ItemProperties,
    pub nbt: CachedNBT,
}

impl PartialEq for ItemStack {
    fn eq(&self, other: &Self) -> bool {
        self.item == other.item && self.count == other.count && self.nbt == other.nbt
    }
}

impl Eq for ItemStack {}

impl ItemStack {
    pub const EMPTY: Self = Self {
        item: Item::Air,
        count: 0,
        properties: Item::Air.get_properties(),
        nbt: CachedNBT::new(),
    };

    pub const fn new(item: Item) -> Self {
        Self {
            item,
            count: 1,
            properties: item.get_properties(),
            nbt: CachedNBT::new()
        }
    }

    pub const fn new_with_count(item: Item, count: i8) -> Self {
        Self {
            item,
            count,
            properties: item.get_properties(),
            nbt: CachedNBT::new()
        }
    }

    pub fn properties(&self) -> &'static ItemProperties {
        self.properties
    }

    pub fn is_empty(&self) -> bool {
        self.item == Item::Air || self.count == 0
    }

    pub fn not_empty(self: Self) -> Option<Self> {
        if self.is_empty() {
            None
        } else {
            Some(self)
        }
    }
}

impl<'a> TryFrom<ProtocolItemStack<'a>> for ItemStack {
    type Error = NoSuchItemError;
    fn try_from(protocol_itemstack: ProtocolItemStack) -> Result<Self, Self::Error> {
        if protocol_itemstack.is_empty() {
            Ok(Self::EMPTY)
        } else {
            let item = Item::try_from(protocol_itemstack.item as u16)?;
            let properties = item.get_properties();
    
            Ok(ItemStack {
                item,
                properties,
                count: protocol_itemstack.count,
                nbt: protocol_itemstack.nbt.into_owned(),
            })
        }
    }
}

impl<'a> From<&'a ItemStack> for ProtocolItemStack<'a> {
    fn from(itemstack: &'a ItemStack) -> Self {
        if itemstack.is_empty() {
            ProtocolItemStack::EMPTY
        } else {
            ProtocolItemStack {
                item: itemstack.item as _,
                count: itemstack.count,
                nbt: Cow::Borrowed(&itemstack.nbt),
            }
        }
    }
}
