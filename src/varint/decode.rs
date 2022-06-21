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

pub fn i32(slice: &[u8]) -> Result<(i32, usize), VarintDecodeOutOfBounds> {
    let len = slice.len();
    if len >= 8 {
        Ok(unsafe { i32_unchecked(slice) } )
    } else {
        let mut data = [0u8; 8];
        data[..len].copy_from_slice(slice);
        let (num, size) = unsafe { i32_unchecked(&data) };

        // Check bounds
        if size > len {
            Err(VarintDecodeOutOfBounds)
        } else {
            Ok((num, size))
        }
    }
}

pub unsafe fn i32_unchecked(slice: &[u8]) -> (i32, usize) {
    debug_assert!(slice.len() >= 8, "invariant: slice must contain at least 8 bytes to decode varint");

    let (num, size) = decode_varint_generic::<u64>(slice.as_ptr(), 5);
    (std::mem::transmute(num as u32), size)
}

// == 3-byte constrained (max representable = u21)

pub fn u21(slice: &[u8]) -> Result<(u32, usize), VarintDecodeOutOfBounds> {
    let len = slice.len();
    if len >= 4 {
        Ok(unsafe { u21_unchecked(slice) })
    } else {
        let mut data = [0u8; 4];
        data[..len].copy_from_slice(slice);
        let (num, size) = unsafe { u21_unchecked(&data) };

        // Check bounds
        if size > len {
            Err(VarintDecodeOutOfBounds)
        } else {
            Ok((num, size))
        }
    }
}

pub unsafe fn u21_unchecked(slice: &[u8]) -> (u32, usize) {
    debug_assert!(slice.len() >= 4, "invariant: slice must contain at least 4 bytes to decode varint");

    let (num, size) = decode_varint_generic::<u32>(slice.as_ptr(), 3);
    (num as u32, size)
}

// == 2-byte constrained (max representable = u14)

pub fn u14(slice: &[u8]) -> Result<(u16, usize), VarintDecodeOutOfBounds> {
    let len = slice.len();
    if len >= 2 {
        Ok(unsafe { u14_unchecked(slice) })
    } else {
        let mut data = [0u8; 2];
        data[..len].copy_from_slice(slice);
        let (num, size) = unsafe { u14_unchecked(&data) };

        // Check bounds
        if size > len {
            Err(VarintDecodeOutOfBounds)
        } else {
            Ok((num, size))
        }
    }
}

pub unsafe fn u14_unchecked(slice: &[u8]) -> (u16, usize) {
    debug_assert!(slice.len() >= 2, "invariant: slice must contain at least 2 bytes to decode varint");

    let (num, size) = decode_varint_generic::<u16>(slice.as_ptr(), 2);
    (num as u16, size)
}

// == manual implementation of varint_generic for max_parts=5 - for reference

/*unsafe fn varint32_unsafe(bytes: *const u8) -> (i32, usize) {
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