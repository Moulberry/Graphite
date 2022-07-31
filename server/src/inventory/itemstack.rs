use minecraft_constants::item::{Item, NoSuchItemError};
use protocol::types::ProtocolItemStack;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ItemStack {
    pub item: Item,
    pub count: i8
}

impl TryFrom<ProtocolItemStack> for ItemStack {
    type Error = NoSuchItemError;
    fn try_from(protocol_itemstack: ProtocolItemStack) -> Result<Self, Self::Error> {
        Ok(ItemStack {
            item: Item::try_from(protocol_itemstack.item as u16)?,
            count: protocol_itemstack.count
        })
    }
}