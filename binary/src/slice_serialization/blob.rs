
use super::*;
pub enum GreedyBlob {}

impl<'a> SliceSerializable<'a, &'a [u8]> for GreedyBlob {
    type RefType = &'a [u8];

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<&'a [u8]> {
        let ret_bytes = *bytes;
        *bytes = &bytes[bytes.len()..];
        Ok(ret_bytes)
    }

    fn get_write_size(data: &[u8]) -> usize {
        data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &[u8], ) -> &'b mut [u8] {
        bytes[0..data.len()].clone_from_slice(data);
        &mut bytes[data.len()..]
    }

    #[inline(always)]
    fn maybe_deref(t: &&'a [u8]) -> Self::RefType {
        *t
    }
}

pub enum SizedBlob<const MAX_SIZE: usize = 2097152, const SIZE_MULT: usize = 1> {}
impl<'a, const MAX_SIZE: usize, const SIZE_MULT: usize> SliceSerializable<'a, &'a [u8]> for SizedBlob<MAX_SIZE, SIZE_MULT> {
    type RefType = &'a [u8];

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<&'a [u8]> {
        let blob_size = VarInt::read(bytes)? as usize;

        // Validate blob byte-length
        if blob_size > MAX_SIZE * SIZE_MULT {
            return Err(BinaryReadError::BlobBytesExceedMaxSize(blob_size, MAX_SIZE * SIZE_MULT).into());
        }
        if blob_size > bytes.len() {
            return Err(BinaryReadError::NotEnoughRemainingBytes.into());
        }

        // Validate utf-8
        let (blob_bytes, rest_bytes) = bytes.split_at(blob_size);
        *bytes = rest_bytes;

        Ok(blob_bytes)
    }

    fn get_write_size(data: &[u8]) -> usize {
        VarInt::get_write_size(data.len() as i32) + data.len()
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], data: &[u8]) -> &'b mut [u8] {
        let len = data.len();

        // 1. write len(blob) as varint header
        bytes = VarInt::write(bytes, len as i32);

        // 2. write blob itself
        debug_assert!(
            bytes.len() >= len,
            "invariant: slice must contain at least 5+len(blob) bytes to perform write"
        );

        // split bytes, write into first, set bytes to remaining
        bytes[..len].clone_from_slice(data);
        &mut bytes[len..]
    }

    fn maybe_deref(t: &&'a [u8]) -> Self::RefType {
        *t
    }
}

pub enum SizedString<const MAX_SIZE: usize = 32767> {}

impl<'a, const MAX_SIZE: usize> SliceSerializable<'a, &'a str> for SizedString<MAX_SIZE> {
    type RefType = &'a str;

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
        VarInt::get_write_size(data.len() as i32) + data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &str) -> &'b mut [u8] {
        SizedBlob::<MAX_SIZE, 4>::write(bytes, data.as_bytes())
    }

    #[inline(always)]
    fn maybe_deref(t: &&'a str) -> Self::RefType {
        *t
    }
}