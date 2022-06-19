use thiserror::Error;

#[derive(Error, Debug)]
#[error("not enough bytes to fully decode varint")]
pub struct VarintDecodeOutOfBounds;

// == fast branchless varint decode

#[inline(always)] // Inline allows for loop unroll & constant folding
unsafe fn decode_varint_generic<R: Into<u64>>(bytes: *const u8, max_parts: usize) -> (u64, usize) {
    // let mask = eg. 0x7fffffffff for max_parts = 5, 0x7fff for max_parts = 2
    let mask: u64 = (0x80 << (max_parts*8-8)) - 1;

    let b: u64 = bytes.cast::<R>().read_unaligned().into() & mask;
    let msbs = !b & !0x7f7f7f7f7f7f7f7f;
    let len = msbs.trailing_zeros() + 1; // in bits
    let varint_part = b & (msbs ^ msbs.wrapping_sub(1));

    let mut num = varint_part & 0x7f;
    for x in 0..max_parts {
        num |= (varint_part & (0x7f << (x * 8))) >> x;
    }

    (num, (len / 8) as usize)
}

// == 5-byte varint

pub fn decode_varint(slice: &[u8]) -> Result<(i32, usize), VarintDecodeOutOfBounds> {
    let len = slice.len();
    if len >= 8 {
        Ok(decode_varint_unchecked(slice))
    } else {
        let mut data = [0u8; 8];
        data[..len].copy_from_slice(slice);
        let (num, size) = decode_varint_unchecked(&data);

        // Check bounds
        if size > len {
            Err(VarintDecodeOutOfBounds)
        } else {
            Ok((num, size))
        }
    }
}

pub fn decode_varint_unchecked(slice: &[u8]) -> (i32, usize) {
    unsafe { decode_varint_unsafe(slice.as_ptr()) }
}

unsafe fn decode_varint_unsafe(bytes: *const u8) -> (i32, usize) {
    let (num, size) = decode_varint_generic::<u64>(bytes, 5);
    (std::mem::transmute(num as u32), size)
}

// == 3-byte constrained (max representable = u21)

pub fn decode_varint3(slice: &[u8]) -> Result<(u32, usize), VarintDecodeOutOfBounds> {
    let len = slice.len();
    if len >= 4 {
        Ok(decode_varint3_unchecked(slice))
    } else {
        let mut data = [0u8; 4];
        data[..len].copy_from_slice(slice);
        let (num, size) = decode_varint3_unchecked(&data);

        // Check bounds
        if size > len {
            Err(VarintDecodeOutOfBounds)
        } else {
            Ok((num, size))
        }
    }
}

pub fn decode_varint3_unchecked(slice: &[u8]) -> (u32, usize) {
    unsafe { decode_varint3_unsafe(slice.as_ptr()) }
}

unsafe fn decode_varint3_unsafe(bytes: *const u8) -> (u32, usize) {
    let (num, size) = decode_varint_generic::<u32>(bytes, 3);
    (num as u32, size)
}

// == 2-byte constrained (max representable = u14)

pub fn decode_varint2(slice: &[u8]) -> Result<(u16, usize), VarintDecodeOutOfBounds> {
    let len = slice.len();
    if len >= 2 {
        Ok(decode_varint2_unchecked(slice))
    } else {
        let mut data = [0u8; 2];
        data[..len].copy_from_slice(slice);
        let (num, size) = decode_varint2_unchecked(&data);

        // Check bounds
        if size > len {
            Err(VarintDecodeOutOfBounds)
        } else {
            Ok((num, size))
        }
    }
}

pub fn decode_varint2_unchecked(slice: &[u8]) -> (u16, usize) {
    unsafe { decode_varint2_unsafe(slice.as_ptr()) }
}

unsafe fn decode_varint2_unsafe(bytes: *const u8) -> (u16, usize) {
    let (num, size) = decode_varint_generic::<u16>(bytes, 2);
    (num as u16, size)
}

// == manual implementation of decode_varint_generic for max_parts=5 - for reference

/*unsafe fn decode_varint32_unsafe(bytes: *const u8) -> (i32, usize) {
    let b = bytes.cast::<u64>().read_unaligned() & 0x7fffffffff;
    let msbs = !b & !0x7f7f7f7f7f7f7f7f;
    let len = msbs.trailing_zeros() + 1; // in bits
    let varint_part = b & (msbs ^ msbs.wrapping_sub(1));

    let num = ((varint_part & 0x000000000000007f)
        | ((varint_part & 0x0000000f00000000) >> 4)
        | ((varint_part & 0x000000007f000000) >> 3)
        | ((varint_part & 0x00000000007f0000) >> 2)
        | ((varint_part & 0x0000000000007f00) >> 1)) as u32;

    (std::mem::transmute(num), (len / 8) as usize)
}*/