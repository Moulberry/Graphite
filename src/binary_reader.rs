use thiserror::Error;

use crate::varint;

const DEFAULT_STRING_MAX_SIZE: usize = 32767;

#[derive(Error, Debug)]
pub enum BinaryReadError {
    #[error("buffer does not contain enough bytes to perform read")]
    NotEnoughRemainingBytes,
    #[error("string byte count ({0}) exceeds maximum ({1})")]
    StringBytesExceedMaxSize(usize, usize),
    #[error("string character count ({0}) exceeds maximum ({1})")]
    StringCharsExceedMaxSize(usize, usize),
    #[error("didn't fully consume packet buffer, {0} byte(s) remained")]
    DidntFullyConsume(usize)
}

pub fn ensure_fully_read(bytes: &[u8]) -> anyhow::Result<()> {
    if bytes.len() == 0 {
        Ok(())
    } else {
        Err(BinaryReadError::DidntFullyConsume(bytes.len()).into())
    }
}

pub fn read_varint(bytes: &mut &[u8]) -> anyhow::Result<i32> {
    if bytes.len() == 0 {
        return Err(BinaryReadError::NotEnoughRemainingBytes.into());
    }

    let (num, size) = varint::decode_varint(bytes)?;
    *bytes = &bytes[size..];
    Ok(num)
}

pub fn read_string<'a>(bytes: &mut &'a[u8]) -> anyhow::Result<&'a str> {
    read_string_with_max_size(bytes, DEFAULT_STRING_MAX_SIZE)
}

pub fn read_string_with_max_size<'a>(bytes: &mut &'a[u8], max_size: usize) -> anyhow::Result<&'a str> {
    if bytes.len() == 0 {
        return Err(BinaryReadError::NotEnoughRemainingBytes.into());
    }

    // Get string length
    let (string_size, consumed) = varint::decode_varint3(bytes)?;
    *bytes = &bytes[consumed..];
    let string_size = string_size as usize;

    // Validate string byte-length
    if string_size > max_size * 4 {
        return Err(BinaryReadError::StringBytesExceedMaxSize(string_size, max_size * 4).into());
    }
    if string_size > bytes.len() {
        return Err(BinaryReadError::NotEnoughRemainingBytes.into());
    }

    // Validate utf-8
    let (string_bytes, rest_bytes) = bytes.split_at(string_size);
    let string = std::str::from_utf8(string_bytes)?;
    *bytes = rest_bytes;

    // Check character count, if necessary
    if string_size as usize > max_size {
        let character_count = string.chars().count();
        if character_count > max_size {
            return Err(BinaryReadError::StringCharsExceedMaxSize(character_count, max_size).into());
        }
    }

    Ok(string)
}

macro_rules! read_from_primitive_impl {
    ($func:ident, $typ:tt::$conv:tt) => {
        pub fn $func(bytes: &mut &[u8]) -> anyhow::Result<$typ> {
            const SIZE: usize = std::mem::size_of::<$typ>();

            if bytes.len() < SIZE {
                return Err(BinaryReadError::NotEnoughRemainingBytes.into());
            }

            // Read value using conversion function
            let ret = unsafe { $typ::$conv(*(bytes as *const _ as *const [_; SIZE])) };

            // Advance
            *bytes = &bytes[SIZE..];

            Ok(ret)
        }
    };
}

read_from_primitive_impl!(read_u16, u16::from_be_bytes);