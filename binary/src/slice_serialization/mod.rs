use thiserror::Error;

pub mod option;

pub mod sized_array;
pub use sized_array::SizedArray;

pub mod single_byte;
pub use single_byte::Single;

pub mod varint;
pub use varint::VarInt;

pub mod blob;
pub use blob::GreedyBlob;
pub use blob::SizedBlob;
pub use blob::SizedString;

pub mod primitive;
pub use primitive::BigEndian;

#[derive(Error, Debug)]
pub enum BinaryReadError {
    #[error("buffer does not contain enough bytes to perform read")]
    NotEnoughRemainingBytes,
    #[error("string byte count ({0}) exceeds maximum ({1})")]
    BlobBytesExceedMaxSize(usize, usize),
    #[error("string character count ({0}) exceeds maximum ({1})")]
    StringCharsExceedMaxSize(usize, usize),
    #[error("didn't fully consume packet buffer, {0} byte(s) remained")]
    DidntFullyConsume(usize),
}

pub trait SliceSerializable<'a, T = Self> {
    type RefType;

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<T>;
    fn get_write_size(data: Self::RefType) -> usize;
    unsafe fn write<'b>(bytes: &'b mut [u8], data: Self::RefType) -> &'b mut [u8];

    fn maybe_deref(t: &'a T) -> Self::RefType;
}

pub fn check_empty(bytes: &[u8]) -> anyhow::Result<()> {
    if bytes.is_empty() {
        Ok(())
    } else {
        Err(BinaryReadError::DidntFullyConsume(bytes.len()).into())
    }
}

// Macro to generate composite slice_serializables

//todo: remove this macro
#[macro_export]
macro_rules! resolve_wire_type {
    ( $typ:ty ) => {
        $typ
    };
    ( $typ:ty as $wire:ty ) => {
        $wire
    };
}

#[macro_export]
macro_rules! slice_serializable_composite {
    /*( @resolve_wire_type $typ:ty => {
        $typ
    };
    { @resolve_wire_type $typ:ty as $wire:ty } => {
        $wire
    };*/
    { $struct_name:ident$(<$lt:lifetime>)?, $( $field_name:ident : $typ:ty $( as $wire:ty )? ),* } => {
        #[derive(Debug)]
        pub struct $struct_name$(<$lt>)? {
            $(pub $field_name: $typ,)*
        }

        impl <'a> SliceSerializable<'a> for $struct_name$(<$lt>)? {
            type RefType = &'a $struct_name$(<$lt>)?;

            fn read(bytes: &mut &$($lt)?[u8]) -> anyhow::Result<$struct_name$(<$lt>)?> {
                Ok($struct_name {
                    $(
                        $field_name: <resolve_wire_type!($typ $( as $wire )?)>::read(bytes)?,
                    )*
                })
            }

            fn get_write_size(object: &$($lt)?$struct_name) -> usize {
                $(
                    <resolve_wire_type!($typ $( as $wire )?) as SliceSerializable<$typ>>::get_write_size(
                        <resolve_wire_type!($typ $( as $wire )?) as SliceSerializable<$typ>>::maybe_deref(&object.$field_name)) +
                )*
                0
            }

            unsafe fn write<'b>(mut bytes: &'b mut [u8], object: &$($lt)?$struct_name) -> &'b mut [u8] {
                $(
                    bytes = <resolve_wire_type!($typ $( as $wire )?) as SliceSerializable<$typ>>::write(bytes, 
                        <resolve_wire_type!($typ $( as $wire )?) as SliceSerializable<$typ>>::maybe_deref(&object.$field_name));
                )*
                bytes
            }

            
            fn maybe_deref(t: &'a $struct_name) -> Self::RefType {
                t
            }
        }
    };
    /*{ $struct_name:ident<$lt:lifetime>, $( $field_name:ident : $typ:ty $( as $wire:ty )? ),* } => {
        #[derive(Debug)]
        pub struct $struct_name<$lt> {
            $(pub $field_name: $typ,)*
        }

        impl <$lt> SliceSerializable<$lt> for $struct_name<$lt> {
            fn read(bytes: &mut &$lt [u8]) -> anyhow::Result<$struct_name<$lt>> {
                Ok($struct_name {
                    $(
                        $field_name: <resolve_wire_type!($typ $( as $wire )?)>::read(bytes)?,
                    )*
                })
            }

            fn get_write_size(object: &$lt$struct_name) -> usize {
                $(
                    <resolve_wire_type!($typ $( as $wire )?)>::get_write_size(object.$field_name) +
                )*
                0
            }

            unsafe fn write<'b>(mut bytes: &'b mut [u8], object: &$lt$struct_name) -> &'b mut [u8] {
                $(
                    bytes = <resolve_wire_type!($typ $( as $wire )?)>::write(bytes, object.$field_name);
                )*
                bytes
            }
        }
    }*/
}

pub use resolve_wire_type;
pub use slice_serializable_composite;
