use std::borrow::Cow;

use crate::nbt::{decode, EncodedNBT};

use super::*;

pub enum NBTBlob {}

impl<'r, 'd: 'r> SliceSerializable<'r, 'd, EncodedNBT> for NBTBlob {
    type CopyType = &'r EncodedNBT;

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<EncodedNBT> {
        // todo: use validate_protocol instead of read_protocol
        let _ = decode::read_protocol(bytes)?;
        Ok(EncodedNBT::new_from_raw_bytes(bytes.to_vec()))
    }

    fn get_write_size(data: &EncodedNBT) -> usize {
        data.to_bytes().len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &EncodedNBT) -> &'b mut [u8] {
        let to_write = data.to_bytes();
        bytes[0..to_write.len()].clone_from_slice(&*to_write);
        &mut bytes[to_write.len()..]
    }

    #[inline(always)]
    fn as_copy_type(t: &'r EncodedNBT) -> Self::CopyType {
        &t
    }
}

pub enum WriteOnlyBlob {}

impl<'r, 'd: 'r> SliceSerializable<'r, 'd, &'d [u8]> for WriteOnlyBlob {
    type CopyType = &'r [u8];

    fn read(_: &mut &'d [u8]) -> anyhow::Result<&'d [u8]> {
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
    fn as_copy_type(t: &'r &'d [u8]) -> Self::CopyType {
        *t
    }
}

impl<'r, 'd: 'r> SliceSerializable<'r, 'd, Box<[u8]>> for WriteOnlyBlob {
    type CopyType = &'r [u8];

    fn read(_: &mut &'d [u8]) -> anyhow::Result<Box<[u8]>> {
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
    fn as_copy_type(t: &'r Box<[u8]>) -> Self::CopyType {
        &*t
    }
}

pub enum GreedyBlob {}

impl<'r, 'd: 'r> SliceSerializable<'r, 'd, Box<[u8]>> for GreedyBlob {
    type CopyType = &'r [u8];

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Box<[u8]>> {
        let ret_bytes = *bytes;
        *bytes = &bytes[bytes.len()..];
        Ok(Box::from(ret_bytes))
    }

    fn get_write_size(data: &[u8]) -> usize {
        data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &[u8]) -> &'b mut [u8] {
        bytes[0..data.len()].clone_from_slice(data);
        &mut bytes[data.len()..]
    }

    #[inline(always)]
    fn as_copy_type(t: &'r Box<[u8]>) -> Self::CopyType {
        &*t
    }
}

impl<'r, 'd: 'r> SliceSerializable<'r, 'd, &'d [u8]> for GreedyBlob {
    type CopyType = &'r [u8];

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<&'d [u8]> {
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
    fn as_copy_type(t: &'r &'d [u8]) -> Self::CopyType {
        *t
    }
}

impl<'r, 'd: 'r> SliceSerializable<'r, 'd, Cow<'d, [u8]>> for GreedyBlob {
    type CopyType = &'r [u8];

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Cow<'d, [u8]>> {
        let ret_bytes = *bytes;
        *bytes = &bytes[bytes.len()..];
        Ok(Cow::Borrowed(ret_bytes))
    }

    fn get_write_size(data: &[u8]) -> usize {
        data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &[u8]) -> &'b mut [u8] {
        bytes[0..data.len()].clone_from_slice(data);
        &mut bytes[data.len()..]
    }

    #[inline(always)]
    fn as_copy_type(t: &'r Cow<'d, [u8]>) -> Self::CopyType {
        t
    }
}

pub enum SizedBlob<const MAX_SIZE: usize = 2097152, const SIZE_MULT: usize = 1> {}
impl<'r, 'd: 'r, const MAX_SIZE: usize, const SIZE_MULT: usize> SliceSerializable<'r, 'd, &'d [u8]>
    for SizedBlob<MAX_SIZE, SIZE_MULT>
{
    type CopyType = &'r [u8];

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<&'d [u8]> {
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
    fn as_copy_type(t: &'r &'d [u8]) -> Self::CopyType {
        *t
    }
}

impl<'r, 'd: 'r, const MAX_SIZE: usize, const SIZE_MULT: usize> SliceSerializable<'r, 'd, Cow<'d, [u8]>>
    for SizedBlob<MAX_SIZE, SIZE_MULT>
{
    type CopyType = &'r [u8];

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Cow<'d, [u8]>> {
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

        Ok(Cow::Borrowed(blob_bytes))
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
    fn as_copy_type(t: &'r Cow<'d, [u8]>) -> Self::CopyType {
        t
    }
}

pub enum FixedBlob<const SIZE: usize> {}
impl<'r, 'd: 'r, const SIZE: usize> SliceSerializable<'r, 'd, &'d [u8]> for FixedBlob<SIZE> {
    type CopyType = &'r [u8];

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<&'d [u8]> {
        if SIZE > bytes.len() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        let (blob_bytes, rest_bytes) = bytes.split_at(SIZE);
        *bytes = rest_bytes;

        Ok(blob_bytes)
    }

    fn get_write_size(_: &[u8]) -> usize {
        SIZE
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &[u8]) -> &'b mut [u8] {
        debug_assert!(
            bytes.len() >= SIZE,
            "invariant: slice must contain at least {} bytes to perform write", SIZE
        );

        bytes[..SIZE].clone_from_slice(data);
        &mut bytes[SIZE..]
    }

    #[inline(always)]
    fn as_copy_type(t: &'r &'d [u8]) -> Self::CopyType {
        *t
    }
}

pub enum StaticSizedString<const MAX_SIZE: usize = 32767> {}

impl<'r, 'd: 'r, const MAX_SIZE: usize> SliceSerializable<'r, 'd, Cow<'static, str>> for StaticSizedString<MAX_SIZE> {
    type CopyType = &'r str;

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Cow<'static, str>> {
        Ok(Cow::Owned(<SizedString<MAX_SIZE> as SliceSerializable<'r, 'd, &'d str>>::read(bytes)?.to_string()))
    }

    fn get_write_size(data: &'r str) -> usize {
        <VarInt as SliceSerializable<usize>>::get_write_size(data.len()) + data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &'r str) -> &'b mut [u8] {
        <SizedString<MAX_SIZE> as SliceSerializable<'r, 'd, &'d str>>::write(bytes, data)
    }

    #[inline(always)]
    fn as_copy_type(t: &'r Cow<'static, str>) -> Self::CopyType {
        t
    }
}

pub enum SizedString<const MAX_SIZE: usize = 32767> {}

impl<'r, 'd: 'r, const MAX_SIZE: usize> SliceSerializable<'r, 'd, &'d str> for SizedString<MAX_SIZE> {
    type CopyType = &'r str;

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<&'d str> {
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
        <SizedBlob::<MAX_SIZE, 4> as SliceSerializable<&[u8]>>::write(bytes, data.as_bytes())
    }

    #[inline(always)]
    fn as_copy_type(t: &'r &'d str) -> Self::CopyType {
        *t
    }
}

impl<'r, 'd: 'r, const MAX_SIZE: usize> SliceSerializable<'r, 'd, Cow<'d, str>> for SizedString<MAX_SIZE> {
    type CopyType = &'r str;

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Cow<'d, str>> {
        Ok(Cow::Borrowed(<SizedString<MAX_SIZE> as SliceSerializable<'r, 'd, &'d str>>::read(bytes)?))
    }

    fn get_write_size(data: &'r str) -> usize {
        <VarInt as SliceSerializable<usize>>::get_write_size(data.len()) + data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &'r str) -> &'b mut [u8] {
        <SizedString<MAX_SIZE> as SliceSerializable<'r, 'd, &'d str>>::write(bytes, data)
    }

    #[inline(always)]
    fn as_copy_type(t: &'r Cow<'d, str>) -> Self::CopyType {
        t
    }
}

impl<'r, 'd: 'r, const MAX_SIZE: usize> SliceSerializable<'r, 'd, String> for SizedString<MAX_SIZE> {
    type CopyType = &'r String;

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<String> {
        Ok(String::from(
            <SizedString<MAX_SIZE> as SliceSerializable<'r, 'd, &'d str>>::read(bytes)?,
        ))
    }

    fn get_write_size(data: &'r String) -> usize {
        <VarInt as SliceSerializable<usize>>::get_write_size(data.len()) + data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &'r String) -> &'b mut [u8] {
        <SizedString<MAX_SIZE> as SliceSerializable<'r, 'd, &'d str>>::write(bytes, data)
    }

    #[inline(always)]
    fn as_copy_type(t: &'r String) -> Self::CopyType {
        t
    }
}
