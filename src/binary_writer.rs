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