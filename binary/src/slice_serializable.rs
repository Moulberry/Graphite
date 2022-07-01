use std::marker::PhantomData;

use crate::slice_reader::BinaryReadError;

pub trait SliceSerializable<'a, T = Self> {
    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<T>;
    fn get_write_size(data: &'a T) -> usize;
    unsafe fn write<'b>(bytes: &'b mut [u8], data: &'a T) -> &'b mut [u8];
}

pub struct SizedArray<'a, S: SliceSerializable<'a, T>, T> {
    a: PhantomData<&'a T>,
    s: PhantomData<S>,
}
impl<'a, T, S: SliceSerializable<'a, T>> SliceSerializable<'a, Vec<T>> for SizedArray<'a, S, T> {
    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<Vec<T>> {
        let size = super::slice_reader::read_varint(bytes)?;

        if size <= 0 {
            return Ok(vec![]);
        }

        let mut vec = Vec::with_capacity(size as usize);

        for _ in 0..size {
            vec.push(S::read(bytes)?);
        }

        Ok(vec)
    }

    fn get_write_size(entries: &'a Vec<T>) -> usize {
        let mut size: usize = VarInt::get_write_size(&(entries.len() as i32));
        for entry in entries {
            size += S::get_write_size(entry);
        }
        size
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], entries: &'a Vec<T>) -> &'b mut [u8] {
        bytes = super::slice_writer::write_varint_i32(bytes, entries.len() as i32);

        for entry in entries {
            bytes = S::write(bytes, entry);
        }

        bytes
    }
}

impl<'a, T, S: SliceSerializable<'a, T>> SliceSerializable<'a, Option<T>> for Option<S> {
    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<Option<T>> {
        let is_present = Single::read(bytes)?;

        if is_present {
            Ok(Option::Some(S::read(bytes)?))
        } else {
            Ok(Option::None)
        }
    }

    fn get_write_size(option: &'a Option<T>) -> usize {
        if let Some(inner) = option {
            1 + S::get_write_size(inner)
        } else {
            1
        }
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], option: &'a Option<T>) -> &'b mut [u8] {
        if let Some(inner) = option {
            bytes = Single::write(bytes, &true);
            bytes = S::write(bytes, inner);
        } else {
            bytes = Single::write(bytes, &false);
        }
        bytes
    }
}

pub enum Single {}
impl SliceSerializable<'_, u8> for Single {
    fn read(bytes: &mut &[u8]) -> anyhow::Result<u8> {
        if bytes.is_empty() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        let ret = bytes[0];
        *bytes = &bytes[1..];
        Ok(ret)
    }

    fn get_write_size(_: &u8) -> usize {
        1
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &u8) -> &'b mut [u8] {
        debug_assert!(
            !bytes.is_empty(),
            "invariant: slice must contain at least 1 byte to perform read"
        );

        bytes[0] = *data;
        &mut bytes[1..]
    }
}
impl SliceSerializable<'_, i8> for Single {
    fn read(bytes: &mut &[u8]) -> anyhow::Result<i8> {
        if bytes.is_empty() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        let ret = unsafe { std::mem::transmute(bytes[0]) };
        *bytes = &bytes[1..];
        Ok(ret)
    }

    fn get_write_size(_: &i8) -> usize {
        1
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &i8) -> &'b mut [u8] {
        debug_assert!(
            !bytes.is_empty(),
            "invariant: slice must contain at least 1 byte to perform read"
        );

        bytes[0] = std::mem::transmute(*data);
        &mut bytes[1..]
    }
}
impl SliceSerializable<'_, bool> for Single {
    fn read(bytes: &mut &[u8]) -> anyhow::Result<bool> {
        if bytes.is_empty() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        let ret = bytes[0] != 0;
        *bytes = &bytes[1..];
        Ok(ret)
    }

    fn get_write_size(_: &bool) -> usize {
        1
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &bool) -> &'b mut [u8] {
        debug_assert!(
            !bytes.is_empty(),
            "invariant: slice must contain at least 1 byte to perform read"
        );

        bytes[0] = if *data { 1 } else { 0 };
        &mut bytes[1..]
    }
}

pub enum VarInt {}
impl SliceSerializable<'_, i32> for VarInt {
    fn read(bytes: &mut &[u8]) -> anyhow::Result<i32> {
        super::slice_reader::read_varint(bytes)
    }

    fn get_write_size(_: &i32) -> usize {
        5 // todo: we could calculate the needed bits as a time->space tradeoff. probably not worth?
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &i32) -> &'b mut [u8] {
        super::slice_writer::write_varint_i32(bytes, *data)
    }
}

pub enum GreedyBlob {}
impl<'a> SliceSerializable<'a, &'a [u8]> for GreedyBlob {
    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<&'a [u8]> {
        let ret_bytes = *bytes;
        *bytes = &bytes[bytes.len()..];
        Ok(ret_bytes)
    }

    fn get_write_size(data: &&[u8]) -> usize {
        data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &&[u8]) -> &'b mut [u8] {
        bytes[0..data.len()].clone_from_slice(*data);
        &mut bytes[data.len()..]
    }
}

pub enum SizedBlob<const MAX_SIZE: usize = 2097152> {}
impl<'a, const MAX_SIZE: usize> SliceSerializable<'a, &'a [u8]> for SizedBlob<MAX_SIZE> {
    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<&'a [u8]> {
        let blob_size = VarInt::read(bytes)? as usize;

        // Validate blob byte-length
        if blob_size > MAX_SIZE {
            return Err(BinaryReadError::BlobBytesExceedMaxSize(blob_size, MAX_SIZE).into());
        }
        if blob_size > bytes.len() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        // Validate utf-8
        let (blob_bytes, rest_bytes) = bytes.split_at(blob_size);
        *bytes = rest_bytes;

        Ok(blob_bytes)
    }

    fn get_write_size(data: &&[u8]) -> usize {
        VarInt::get_write_size(&(data.len() as i32)) + data.len()
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], data: &&[u8]) -> &'b mut [u8] {
        let len = data.len();

        // 1. write len(blob) as varint header
        bytes = VarInt::write(bytes, &(len as i32));

        // 2. write blob itself
        debug_assert!(
            bytes.len() >= len,
            "invariant: slice must contain at least 5+len(blob) bytes to perform write"
        );

        // split bytes, write into first, set bytes to remaining
        bytes[..len].clone_from_slice(data);
        &mut bytes[len..]
    }
}

pub enum SizedString<const MAX_SIZE: usize = 32767> {}
impl<'a, const MAX_SIZE: usize> SliceSerializable<'a, &'a str> for SizedString<MAX_SIZE> {
    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<&'a str> {
        super::slice_reader::read_string_with_max_size(bytes, MAX_SIZE)
    }

    fn get_write_size(data: &&str) -> usize {
        VarInt::get_write_size(&(data.len() as i32)) + data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &&str) -> &'b mut [u8] {
        super::slice_writer::write_sized_string(bytes, data)
    }
}

pub enum BigEndian {}

macro_rules! for_primitive {
    ($typ:tt, $mode:ident, $conv_from:tt, $conv_to:tt) => {
        impl SliceSerializable<'_, $typ> for $mode {
            fn read(bytes: &mut &[u8]) -> anyhow::Result<$typ> {
                const SIZE: usize = std::mem::size_of::<$typ>();

                if bytes.len() < SIZE {
                    return Err(BinaryReadError::NotEnoughRemainingBytes.into());
                }

                // Read value using conversion function
                let ret = unsafe { $typ::$conv_from(*(bytes as *const _ as *const [_; SIZE])) };

                // Advance
                *bytes = &bytes[SIZE..];

                Ok(ret)
            }

            fn get_write_size(_: &$typ) -> usize {
                std::mem::size_of::<$typ>()
            }

            unsafe fn write<'b>(bytes: &'b mut [u8], primitive: &$typ) -> &'b mut [u8] {
                const SIZE: usize = std::mem::size_of::<$typ>();

                debug_assert!(
                    bytes.len() >= SIZE,
                    "invariant: slice must contain at least {} bytes to perform $func",
                    SIZE
                );

                bytes[..SIZE].clone_from_slice(&$typ::$conv_to(*primitive));
                &mut bytes[SIZE..]
            }
        }
    };
}

for_primitive!(u16, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(u64, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(u128, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(i32, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(i64, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(f32, BigEndian, from_be_bytes, to_be_bytes);
for_primitive!(f64, BigEndian, from_be_bytes, to_be_bytes);

// Macro to generate composite slice_serializables

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
    { $struct_name:ident, $( $field_name:ident : $typ:ty $( as $wire:ty )? ),* } => {
        #[derive(Debug)]
        pub struct $struct_name {
            $(pub $field_name: $typ,)*
        }

        impl SliceSerializable<'_> for $struct_name {
            fn read(bytes: &mut &[u8]) -> anyhow::Result<$struct_name> {
                Ok($struct_name {
                    $(
                        $field_name: <resolve_wire_type!($typ $( as $wire )?)>::read(bytes)?,
                    )*
                })
            }

            fn get_write_size(object: &$struct_name) -> usize {
                $(
                    <resolve_wire_type!($typ $( as $wire )?)>::get_write_size(&object.$field_name) +
                )*
                0
            }

            unsafe fn write<'b>(mut bytes: &'b mut [u8], object: &$struct_name) -> &'b mut [u8] {
                $(
                    bytes = <resolve_wire_type!($typ $( as $wire )?)>::write(bytes, &object.$field_name);
                )*
                bytes
            }
        }
    };
    { $struct_name:ident<$lt:lifetime>, $( $field_name:ident : $typ:ty $( as $wire:ty )? ),* } => {
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
                    <resolve_wire_type!($typ $( as $wire )?)>::get_write_size(&object.$field_name) +
                )*
                0
            }

            unsafe fn write<'b>(mut bytes: &'b mut [u8], object: &$lt$struct_name) -> &'b mut [u8] {
                $(
                    bytes = <resolve_wire_type!($typ $( as $wire )?)>::write(bytes, &object.$field_name);
                )*
                bytes
            }
        }
    }
}

pub use resolve_wire_type;
pub use slice_serializable_composite;
