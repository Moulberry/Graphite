use std::borrow::Cow;

use binary::nbt::CachedNBT;
use minecraft_constants::item::{Item, ItemProperties, NoSuchItemError};
use protocol::types::ProtocolItemStack;

#[derive(Clone, Debug)]
pub struct ItemStack {
    pub(crate) item: Item,
    pub(crate) count: i8,
    pub(crate) properties: &'static ItemProperties,
    pub(crate) nbt: CachedNBT,
}

impl PartialEq for ItemStack {
    fn eq(&self, other: &Self) -> bool {
        self.item == other.item && self.count == other.count
    }
}

impl<'a> TryFrom<ProtocolItemStack<'a>> for ItemStack {
    type Error = NoSuchItemError;
    fn try_from(protocol_itemstack: ProtocolItemStack) -> Result<Self, Self::Error> {
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

impl<'a> From<&'a ItemStack> for ProtocolItemStack<'a> {
    fn from(itemstack: &'a ItemStack) -> Self {
        ProtocolItemStack {
            item: itemstack.item as _,
            count: itemstack.count,
            nbt: Cow::Borrowed(&itemstack.nbt),
        }
    }
}
