pub fn extend_i32(vec: &mut Vec<u8>, num: i32) {
    let (bytes, size) = i32_raw(num);
    vec.extend_from_slice(&bytes[..size]);
}

pub fn i32_raw(num: i32) -> ([u8; 8], usize) {
    let x = unsafe { std::mem::transmute::<i32, u32>(num) } as u64;
    let stage1 = (x & 0x000000000000007f)
        | ((x & 0x0000000000003f80) << 1)
        | ((x & 0x00000000001fc000) << 2)
        | ((x & 0x000000000fe00000) << 3)
        | ((x & 0x00000000f0000000) << 4);

    let leading = stage1.leading_zeros();

    let unused_bytes = (leading - 1) / 8;
    let bytes_needed = 8 - unused_bytes;

    // set all but the last MSBs
    let msbs = 0x8080808080808080;
    let msbmask = 0xFFFFFFFFFFFFFFFF >> ((8 - bytes_needed + 1) * 8 - 1);

    let merged = stage1 | (msbs & msbmask);

    (
        unsafe { std::mem::transmute([merged]) },
        bytes_needed as usize,
    )
}

pub fn needed_bytes(num: i32) -> usize {
    if num == 0 {
        1
    } else {
        (31 - num.leading_zeros() as usize) / 7 + 1
    }
}
