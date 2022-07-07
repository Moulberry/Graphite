use std::marker::PhantomData;

use super::*;

pub struct SizedArray<S> {
    _a: PhantomData<S>
}

impl<'a, T: 'a, S: SliceSerializable<'a, T>> SliceSerializable<'a, Vec<T>> for SizedArray<S> {
    type RefType = &'a Vec<T>;

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<Vec<T>> {
        let array_length = VarInt::read(bytes)? as usize;

        if array_length <= 0 {
            return Ok(vec![]);
        }

        let mut vec = Vec::with_capacity(array_length as usize);
        for _ in 0..array_length {
            vec.push(S::read(bytes)?);
        }

        Ok(vec)
    }

    fn get_write_size(entries: &'a Vec<T>) -> usize {
        let mut size: usize = VarInt::get_write_size(entries.len() as i32);
        for entry in entries {
            size += S::get_write_size(S::maybe_deref(entry));
        }
        size
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], entries: &'a Vec<T>) -> &'b mut [u8] {
        bytes = VarInt::write(bytes, entries.len() as i32);
        for entry in entries {
            bytes = S::write(bytes, S::maybe_deref(entry));
        }
        bytes
    }
    
    #[inline(always)]
    fn maybe_deref(t: &'a Vec<T>) -> Self::RefType {
        t
    }
}