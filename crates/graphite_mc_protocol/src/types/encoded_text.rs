use std::{fmt::Debug, sync::Arc};

use graphite_binary::{nbt::{decode, EncodedNBT}, slice_serialization::{NBTBlob, SliceSerializable}};

use super::text::{IntoTextComponent, TextComponent};

#[derive(Clone)]
pub struct CachedTextComponent {
    pub(crate) text: Arc<TextComponent<'static>>,
    pub(crate) encoded: EncodedNBT
}

impl CachedTextComponent {
    pub fn to_encoded_nbt(&self) -> EncodedNBT {
        self.encoded.clone()
    }
}

impl IntoTextComponent<'static> for &CachedTextComponent {
    fn into_text_component(self) -> TextComponent<'static> {
        (*self.text).clone()
    }
}

impl From<TextComponent<'static>> for CachedTextComponent {
    fn from(value: TextComponent<'static>) -> Self {
        let encoded = value.to_encoded_nbt();
        Self {
            text: Arc::new(value),
            encoded,
        }
    }
}

impl From<CachedTextComponent> for EncodedNBT {
    fn from(value: CachedTextComponent) -> Self {
        value.to_encoded_nbt()
    }
}

impl PartialEq for CachedTextComponent {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.text, &other.text) || EncodedNBT::ptr_eq(&self.encoded, &other.encoded) || self.text == other.text
    }
}

impl Debug for CachedTextComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.text, f)
    }
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd> for CachedTextComponent {
    type CopyType = &'r CachedTextComponent;

    fn as_copy_type(t: &'r CachedTextComponent) -> Self::CopyType {
        t
    }

    fn read(bytes: &mut &[u8]) -> anyhow::Result<CachedTextComponent> {
        let nbt = decode::read_protocol(bytes)?;
        let text = TextComponent::from_nbt(nbt.as_reference())?;

        Ok(Self {
            text: Arc::new(text),
            encoded: nbt.into(),
        })
    }

    unsafe fn write(bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        NBTBlob::write(bytes, &data.encoded)
    }

    fn get_write_size(data: &'r CachedTextComponent) -> usize {
        NBTBlob::get_write_size(&data.encoded)
    }
}