use std::borrow::Cow;

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
pub use blob::FixedBlob;
pub use blob::StaticSizedString;
pub use blob::SizedString;
pub use blob::WriteOnlyBlob;

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

pub trait SliceSerializable<'r, 'd: 'r, T = Self>: Sized {
    type CopyType: Copy;
    fn as_copy_type(t: &'r T) -> Self::CopyType;

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<T>;
    fn read_fully(bytes: &mut &'d [u8]) -> anyhow::Result<T> {
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

// Default implementation for Cow<SliceSerializable>
impl <'r, 'd: 'r, T: Clone + SliceSerializable<'r, 'd, T>> SliceSerializable<'r, 'd, Cow<'d, T>> for Cow<'d, T> {
    type CopyType = &'r T;

    fn as_copy_type(t: &'r Cow<'r, T>) -> Self::CopyType {
        t
    }

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Cow<'d, T>> {
        Ok(Cow::Owned(T::read(bytes)?))
    }

    unsafe fn write(bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        T::write(bytes, T::as_copy_type(data))
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        T::get_write_size(T::as_copy_type(data))
    }
}

// Default implementation for Box<SliceSerializable>
impl <'r, 'd: 'r, T: Clone + SliceSerializable<'r, 'd, T> + 'r> SliceSerializable<'r, 'd, Box<T>> for Box<T> {
    type CopyType = &'r T;

    fn as_copy_type(t: &'r Box<T>) -> Self::CopyType {
        &*t
    }

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Box<T>> {
        Ok(Box::new(T::read(bytes)?))
    }

    unsafe fn write(bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        T::write(bytes, T::as_copy_type(data))
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        T::get_write_size(T::as_copy_type(data))
    }
}

// Macro to generate composite slice_serializables

pub use graphite_binary_macros::slice_serializable;
