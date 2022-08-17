use thiserror::Error;

mod option;

mod sized_array;
pub use sized_array::SizedArray;

mod single_byte;
pub use single_byte::Single;

mod varint;
pub use varint::VarInt;

mod blob;
pub use blob::GreedyBlob;
pub use blob::NBTBlob;
pub use blob::SizedBlob;
pub use blob::SizedString;

mod primitive;
pub use primitive::BigEndian;
pub use primitive::LittleEndian;

mod from;
pub use from::AttemptFrom;

#[derive(Error, Debug)]
pub enum BinaryReadError {
    #[error("buffer does not contain enough bytes to perform read")]
    NotEnoughRemainingBytes,
    #[error("string byte count ({0}) exceeds maximum ({1})")]
    BlobBytesExceedMaxSize(usize, usize),
    #[error("string character count ({0}) exceeds maximum ({1})")]
    StringCharsExceedMaxSize(usize, usize),
    #[error("didn't fully consume buffer, {0} byte(s) remained")]
    DidntFullyConsume(usize),
}

pub trait SliceSerializable<'a, T = Self> {
    type CopyType: Copy;
    fn as_copy_type(t: &'a T) -> Self::CopyType;

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<T>;
    fn read_fully(bytes: &mut &'a [u8]) -> anyhow::Result<T> {
        let serialized = Self::read(bytes)?;

        if bytes.is_empty() {
            Ok(serialized)
        } else {
            Err(BinaryReadError::DidntFullyConsume(bytes.len()).into())
        }
    }

    /// # Safety
    /// Caller must guarantee that `bytes` contains at least `get_write_size` bytes
    unsafe fn write(bytes: &mut [u8], data: Self::CopyType) -> &mut [u8];
    fn get_write_size(data: Self::CopyType) -> usize;
}

// Macro to generate composite slice_serializables

/*#[macro_export]
macro_rules! slice_serializable_composite {
    ( @resolve_wire_type $typ:ty ) => {
        $typ
    };
    { @resolve_wire_type $typ:ty as $wire:ty } => {
        $wire
    };
    { $struct_name:ident$(<$lt:lifetime>)?, $( $field_name:ident : $typ:ty $( as $wire:ty )? ),* $(,)?} => {
        #[derive(Debug)]
        pub struct $struct_name$(<$lt>)? {
            $(pub $field_name: $typ,)*
        }

        impl <'a> SliceSerializable<'a> for $struct_name$(<$lt>)? {
            type RefType = &'a $struct_name$(<$lt>)?;

            fn read(bytes: &mut &$($lt)?[u8]) -> anyhow::Result<$struct_name$(<$lt>)?> {
                Ok($struct_name {
                    $(
                        $field_name: <slice_serializable_composite!(@resolve_wire_type $typ $( as $wire )?)>::read(bytes)?,
                    )*
                })
            }

            fn get_write_size(object: &$($lt)?$struct_name) -> usize {
                $(
                    <slice_serializable_composite!(@resolve_wire_type $typ $( as $wire )?) as SliceSerializable<$typ>>::get_write_size(
                        <slice_serializable_composite!(@resolve_wire_type $typ $( as $wire )?) as SliceSerializable<$typ>>::maybe_deref(&object.$field_name)) +
                )*
                0
            }

            unsafe fn write<'b>(mut bytes: &'b mut [u8], object: &$($lt)?$struct_name) -> &'b mut [u8] {
                $(
                    bytes = <slice_serializable_composite!(@resolve_wire_type $typ $( as $wire )?) as SliceSerializable<$typ>>::write(bytes,
                        <slice_serializable_composite!(@resolve_wire_type $typ $( as $wire )?) as SliceSerializable<$typ>>::maybe_deref(&object.$field_name));
                )*
                bytes
            }


            fn maybe_deref(t: &'a $struct_name) -> Self::RefType {
                t
            }
        }
    };
}*/

// pub use slice_serializable_composite;
pub use macros::slice_serializable;

/*#[macro_export]
macro_rules! slice_serializable_enum {
    ( @resolve_wire_type $typ:ty ) => {
        $typ
    };
    { @resolve_wire_type $typ:ty as $wire:ty } => {
        $wire
    };
    { $struct_name:ident$(<$lt:lifetime>)?, $( $variant:ident { $( $field_name:ident : $typ:ty $( as $wire:ty )? ),* $(,)? } ),* $(,)?} => {
        #[derive(Debug)]
        pub enum $struct_name$(<$lt>)? {
            $($variant {
                $(pub $field_name: $typ,)*
            })*
        }

        impl <'a> SliceSerializable<'a> for $struct_name$(<$lt>)? {
            type RefType = &'a $struct_name$(<$lt>)?;

            fn read(bytes: &mut &$($lt)?[u8]) -> anyhow::Result<$struct_name$(<$lt>)?> {
                let discriminant = <Single as SliceSerializable<u8>>::read(bytes)?;

                Ok($struct_name {
                    $(
                        $field_name: <slice_serializable_composite!(@resolve_wire_type $typ $( as $wire )?)>::read(bytes)?,
                    )*
                })
            }

            fn get_write_size(object: &$($lt)?$struct_name) -> usize {
                $(
                    <slice_serializable_composite!(@resolve_wire_type $typ $( as $wire )?) as SliceSerializable<$typ>>::get_write_size(
                        <slice_serializable_composite!(@resolve_wire_type $typ $( as $wire )?) as SliceSerializable<$typ>>::maybe_deref(&object.$field_name)) +
                )*
                0
            }

            unsafe fn write<'b>(mut bytes: &'b mut [u8], object: &$($lt)?$struct_name) -> &'b mut [u8] {
                $(
                    bytes = <slice_serializable_composite!(@resolve_wire_type $typ $( as $wire )?) as SliceSerializable<$typ>>::write(bytes,
                        <slice_serializable_composite!(@resolve_wire_type $typ $( as $wire )?) as SliceSerializable<$typ>>::maybe_deref(&object.$field_name));
                )*
                bytes
            }


            fn maybe_deref(t: &'a $struct_name) -> Self::RefType {
                t
            }
        }
    };
}

pub use slice_serializable_composite;*/
