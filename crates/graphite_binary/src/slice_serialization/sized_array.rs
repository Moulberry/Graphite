use std::{marker::PhantomData, borrow::Cow};

use super::*;

pub struct SizedArray<S, const MAX_SIZE: usize = {usize::MAX}> {
    _a: PhantomData<S>,
}

impl<'r, 'd: 'r, const MAX_SIZE: usize, T: 'd, S: SliceSerializable<'r, 'd, T>> SliceSerializable<'r, 'd, Vec<T>> for SizedArray<S, MAX_SIZE> {
    type CopyType = &'r Vec<T>;

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Vec<T>> {
        let array_length: usize = VarInt::read(bytes)?;
        let array_length = array_length.min(MAX_SIZE);

        if array_length == 0 {
            return Ok(vec![]);
        }

        let mut vec = Vec::with_capacity(array_length.min(65536));
        for _ in 0..array_length {
            vec.push(S::read(bytes)?);
        }

        Ok(vec)
    }

    fn get_write_size(entries: &'r Vec<T>) -> usize {
        let mut size: usize = <VarInt as SliceSerializable<usize>>::get_write_size(entries.len());
        for entry in entries {
            size += S::get_write_size(S::as_copy_type(entry));
        }
        size
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], entries: &'r Vec<T>) -> &'b mut [u8] {
        bytes = <VarInt as SliceSerializable<usize>>::write(bytes, entries.len());
        for entry in entries {
            bytes = S::write(bytes, S::as_copy_type(entry));
        }
        bytes
    }

    #[inline(always)]
    fn as_copy_type(t: &'r Vec<T>) -> Self::CopyType {
        t
    }
}

impl<'r, 'd: 'r, T: 'd, S: SliceSerializable<'r, 'd, T>> SliceSerializable<'r, 'd, Cow<'d, [T]>> for SizedArray<S>
where
    [T]: ToOwned<Owned = Vec<T>>,
{
    type CopyType = &'r [T];

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Cow<'d, [T]>> {
        let array_length: usize = VarInt::read(bytes)?;

        if array_length == 0 {
            return Ok(Cow::Owned(vec![]));
        }

        let mut vec = Vec::with_capacity(array_length.min(65536));
        for _ in 0..array_length {
            vec.push(S::read(bytes)?);
        }

        Ok(Cow::Owned(vec))
    }

    fn get_write_size(entries: &'r [T]) -> usize {
        let mut size: usize = <VarInt as SliceSerializable<usize>>::get_write_size(entries.len());
        for entry in entries {
            size += S::get_write_size(S::as_copy_type(entry));
        }
        size
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], entries: &'r [T]) -> &'b mut [u8] {
        bytes = <VarInt as SliceSerializable<usize>>::write(bytes, entries.len());
        for entry in entries {
            bytes = S::write(bytes, S::as_copy_type(entry));
        }
        bytes
    }

    #[inline(always)]
    fn as_copy_type(t: &'r Cow<'d, [T]>) -> Self::CopyType {
        t
    }
}
