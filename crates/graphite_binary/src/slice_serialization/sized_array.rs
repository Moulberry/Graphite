use std::marker::PhantomData;

use super::*;

pub struct SizedArray<S> {
    _a: PhantomData<S>,
}

impl<'a, T: 'a, S: SliceSerializable<'a, T>> SliceSerializable<'a, Vec<T>> for SizedArray<S> {
    type CopyType = &'a Vec<T>;

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<Vec<T>> {
        let array_length: usize = VarInt::read(bytes)?;

        if array_length == 0 {
            return Ok(vec![]);
        }

        let mut vec = Vec::with_capacity(array_length as usize);
        for _ in 0..array_length {
            vec.push(S::read(bytes)?);
        }

        Ok(vec)
    }

    fn get_write_size(entries: &'a Vec<T>) -> usize {
        let mut size: usize = <VarInt as SliceSerializable<usize>>::get_write_size(entries.len());
        for entry in entries {
            size += S::get_write_size(S::as_copy_type(entry));
        }
        size
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], entries: &'a Vec<T>) -> &'b mut [u8] {
        bytes = <VarInt as SliceSerializable<usize>>::write(bytes, entries.len());
        for entry in entries {
            bytes = S::write(bytes, S::as_copy_type(entry));
        }
        bytes
    }

    #[inline(always)]
    fn as_copy_type(t: &'a Vec<T>) -> Self::CopyType {
        t
    }
}
