pub trait BinaryWritable {
    fn put_varint_i32(&mut self, num: i32);
    fn put_sized_string(&mut self, string: &str);
}

impl BinaryWritable for Vec<u8> {
    fn put_varint_i32(&mut self, num: i32) {
        crate::varint::encode::extend_i32(self, num);
    }

    fn put_sized_string(&mut self, string: &str) {
        self.put_varint_i32(string.len() as i32);
        self.extend_from_slice(string.as_bytes());
    }
}

pub unsafe fn write_varint_i32(bytes: &mut [u8], num: i32) -> &mut [u8] {
    debug_assert!(bytes.len() >= 5, "invariant: slice must contain at least 5 bytes to perform varint_i32 write");

    let (encoded, size) = crate::varint::encode::i32_raw(num);
    bytes[..size].clone_from_slice(&encoded[..size]);
    &mut bytes[size..]
}

pub unsafe fn write_sized_string<'a>(mut bytes: &'a mut [u8], string: &str) -> &'a mut [u8] {
    let len = string.len();

    // 1. write len(str) as varint header
    bytes = write_varint_i32(bytes, len as i32);

    // 2. write str itself
    debug_assert!(bytes.len() >= len, "invariant: slice must contain at least 5+len(str) bytes to perform sized_string write");

    // split bytes, write into first, set bytes to remaining
    bytes[..len].clone_from_slice(string.as_bytes());
    &mut bytes[len..]
}

macro_rules! write_from_primitive_impl {
    ($func:ident, $typ:tt::$conv:tt) => {
        pub unsafe fn $func(bytes: &mut [u8], primitive: $typ) -> &mut [u8] {
            const SIZE: usize = std::mem::size_of::<$typ>();

            debug_assert!(bytes.len() >= SIZE, "invariant: slice must contain at least {} bytes to perform $func", SIZE);

            bytes[..SIZE].clone_from_slice(&$typ::$conv(primitive));
            &mut bytes[SIZE..]
        }
    };
}

write_from_primitive_impl!(write_u16, u16::to_be_bytes);