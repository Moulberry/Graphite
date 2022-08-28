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

pub use binary_macros::slice_serializable;
