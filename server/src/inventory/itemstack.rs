use minecraft_constants::item::{Item, ItemProperties, NoSuchItemError};
use protocol::types::ProtocolItemStack;

#[derive(Clone, Debug)]
pub struct ItemStack {
    pub(crate) item: Item,
    pub(crate) count: i8,
    pub(crate) properties: &'static ItemProperties,
}

impl PartialEq for ItemStack {
    fn eq(&self, other: &Self) -> bool {
        self.item == other.item && self.count == other.count
    }
}

impl TryFrom<ProtocolItemStack> for ItemStack {
    type Error = NoSuchItemError;
    fn try_from(protocol_itemstack: ProtocolItemStack) -> Result<Self, Self::Error> {
        let item = Item::try_from(protocol_itemstack.item as u16)?;
        let properties = item.get_properties();

        Ok(ItemStack {
            item,
            properties,
            count: protocol_itemstack.count,
        })
    }
}

impl From<&ItemStack> for ProtocolItemStack {
    fn from(itemstack: &ItemStack) -> Self {
        ProtocolItemStack {
            item: itemstack.item as _,
            count: itemstack.count,
            temp_nbt: 0, // todo: implement nbt
        }
    }
}
