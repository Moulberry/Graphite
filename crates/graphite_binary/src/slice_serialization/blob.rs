use std::borrow::Cow;

use crate::nbt::{decode, CachedNBT};

use super::*;

pub enum NBTBlob {}

impl<'a> SliceSerializable<'a, Cow<'a, CachedNBT>> for NBTBlob {
    type CopyType = &'a CachedNBT;

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<Cow<'a, CachedNBT>> {
        let nbt = decode::read(bytes)?;
        Ok(Cow::Owned(nbt.into()))
    }

    fn get_write_size(data: &CachedNBT) -> usize {
        data.to_bytes().len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &CachedNBT) -> &'b mut [u8] {
        let to_write = data.to_bytes();
        bytes[0..to_write.len()].clone_from_slice(to_write);
        &mut bytes[to_write.len()..]
    }

    #[inline(always)]
    fn as_copy_type(t: &'a Cow<'a, CachedNBT>) -> Self::CopyType {
        t
    }
}

pub enum WriteOnlyBlob {}

impl<'a> SliceSerializable<'a, &'a [u8]> for WriteOnlyBlob {
    type CopyType = &'a [u8];

    fn read(_: &mut &'a [u8]) -> anyhow::Result<&'a [u8]> {
        panic!("tried to read a WriteOnlyBlob");
    }

    fn get_write_size(data: &[u8]) -> usize {
        data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &[u8]) -> &'b mut [u8] {
        bytes[0..data.len()].clone_from_slice(data);
        &mut bytes[data.len()..]
    }

    #[inline(always)]
    fn as_copy_type(t: &&'a [u8]) -> Self::CopyType {
        *t
    }
}

pub enum GreedyBlob {}

impl<'a> SliceSerializable<'a, &'a [u8]> for GreedyBlob {
    type CopyType = &'a [u8];

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<&'a [u8]> {
        let ret_bytes = *bytes;
        *bytes = &bytes[bytes.len()..];
        Ok(ret_bytes)
    }

    fn get_write_size(data: &[u8]) -> usize {
        data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &[u8]) -> &'b mut [u8] {
        bytes[0..data.len()].clone_from_slice(data);
        &mut bytes[data.len()..]
    }

    #[inline(always)]
    fn as_copy_type(t: &&'a [u8]) -> Self::CopyType {
        *t
    }
}

pub enum SizedBlob<const MAX_SIZE: usize = 2097152, const SIZE_MULT: usize = 1> {}
impl<'a, const MAX_SIZE: usize, const SIZE_MULT: usize> SliceSerializable<'a, &'a [u8]>
    for SizedBlob<MAX_SIZE, SIZE_MULT>
{
    type CopyType = &'a [u8];

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<&'a [u8]> {
        let blob_size: usize = VarInt::read(bytes)?;

        // Validate blob byte-length
        if blob_size > MAX_SIZE * SIZE_MULT {
            return Err(
                BinaryReadError::BlobBytesExceedMaxSize(blob_size, MAX_SIZE * SIZE_MULT).into(),
            );
        }
        if blob_size > bytes.len() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        let (blob_bytes, rest_bytes) = bytes.split_at(blob_size);
        *bytes = rest_bytes;

        Ok(blob_bytes)
    }

    fn get_write_size(data: &[u8]) -> usize {
        <VarInt as SliceSerializable<usize>>::get_write_size(data.len()) + data.len()
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], data: &[u8]) -> &'b mut [u8] {
        let len = data.len();

        // 1. write len(blob) as varint header
        bytes = <VarInt as SliceSerializable<usize>>::write(bytes, len);

        // 2. write blob itself
        debug_assert!(
            bytes.len() >= len,
            "invariant: slice must contain at least 5+len(blob) bytes to perform write"
        );

        // split bytes, write into first, set bytes to remaining
        bytes[..len].clone_from_slice(data);
        &mut bytes[len..]
    }

    #[inline(always)]
    fn as_copy_type(t: &&'a [u8]) -> Self::CopyType {
        *t
    }
}

pub enum SizedString<const MAX_SIZE: usize = 32767> {}

impl<'a, const MAX_SIZE: usize> SliceSerializable<'a, &'a str> for SizedString<MAX_SIZE> {
    type CopyType = &'a str;

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<&'a str> {
        let string_bytes = SizedBlob::<MAX_SIZE, 4>::read(bytes)?;

        // Validate utf-8
        let string = std::str::from_utf8(string_bytes)?;

        // Check character count, if necessary
        if string_bytes.len() > MAX_SIZE {
            let character_count = string.chars().count();
            if character_count > MAX_SIZE {
                return Err(
                    BinaryReadError::StringCharsExceedMaxSize(character_count, MAX_SIZE).into(),
                );
            }
        }

        Ok(string)
    }

    fn get_write_size(data: &str) -> usize {
        <VarInt as SliceSerializable<usize>>::get_write_size(data.len()) + data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &str) -> &'b mut [u8] {
        SizedBlob::<MAX_SIZE, 4>::write(bytes, data.as_bytes())
    }

    #[inline(always)]
    fn as_copy_type(t: &&'a str) -> Self::CopyType {
        *t
    }
}

impl<'a, const MAX_SIZE: usize> SliceSerializable<'a, Cow<'a, str>> for SizedString<MAX_SIZE> {
    type CopyType = &'a str;

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<Cow<'a, str>> {
        Ok(Cow::Borrowed(<SizedString<MAX_SIZE> as SliceSerializable<'a, &'a str>>::read(bytes)?))
    }

    fn get_write_size(data: &'a str) -> usize {
        <VarInt as SliceSerializable<usize>>::get_write_size(data.len()) + data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &'a str) -> &'b mut [u8] {
        <SizedString<MAX_SIZE> as SliceSerializable<'a, &'a str>>::write(bytes, data)
    }

    #[inline(always)]
    fn as_copy_type(t: &'a Cow<'a, str>) -> Self::CopyType {
        t
    }
}

impl<'a, const MAX_SIZE: usize> SliceSerializable<'a, String> for SizedString<MAX_SIZE> {
    type CopyType = &'a String;

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<String> {
        Ok(String::from(
            <SizedString<MAX_SIZE> as SliceSerializable<'a, &'a str>>::read(bytes)?,
        ))
    }

    fn get_write_size(data: &'a String) -> usize {
        <VarInt as SliceSerializable<usize>>::get_write_size(data.len()) + data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &'a String) -> &'b mut [u8] {
        <SizedString<MAX_SIZE> as SliceSerializable<'a, &'a str>>::write(bytes, data)
    }

    #[inline(always)]
    fn as_copy_type(t: &'a String) -> Self::CopyType {
        t
    }
}
