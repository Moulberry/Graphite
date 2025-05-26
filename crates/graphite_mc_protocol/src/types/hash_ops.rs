use std::hash::Hasher;

use crc32c::Crc32cHasher;

pub struct HashOps;

impl HashOps {
    pub fn hash_empty() -> i32 {
        let mut hasher = Crc32cHasher::default();
        hasher.write_i8(1);
        hasher.finish() as i32
    }

    pub fn start_map() -> HashOpsMap {
        HashOpsMap { values: Vec::new() }
    }

    pub fn start_list() -> HashOpsList {
        let mut hasher = Crc32cHasher::default();
        hasher.write_i8(4);
        HashOpsList {
            hasher,
        }
    }

    pub fn hash_byte(value: i8) -> i32 {
        let mut hasher = Crc32cHasher::default();
        hasher.write_i8(6);
        hasher.write_i8(value);
        hasher.finish() as i32
    }

    pub fn hash_short(value: i16) -> i32 {
        let mut hasher = Crc32cHasher::default();
        hasher.write_i8(7);
        hasher.write(&value.to_le_bytes());
        hasher.finish() as i32
    }

    pub fn hash_int(value: i32) -> i32 {
        let mut hasher = Crc32cHasher::default();
        hasher.write_i8(8);
        hasher.write(&value.to_le_bytes());
        hasher.finish() as i32
    }

    pub fn hash_long(value: i64) -> i32 {
        let mut hasher = Crc32cHasher::default();
        hasher.write_i8(9);
        hasher.write(&value.to_le_bytes());
        hasher.finish() as i32
    }

    pub fn hash_float(value: f32) -> i32 {
        let mut hasher = Crc32cHasher::default();
        hasher.write_i8(10);
        hasher.write(&value.to_le_bytes());
        hasher.finish() as i32
    }

    pub fn hash_double(value: f64) -> i32 {
        let mut hasher = Crc32cHasher::default();
        hasher.write_i8(11);
        hasher.write(&value.to_le_bytes());
        hasher.finish() as i32
    }

    pub fn hash_string(value: &str) -> i32 {
        let mut hasher = Crc32cHasher::default();
        hasher.write_i8(12);
        hasher.write(&(value.chars().count() as i32).to_le_bytes());
        for char in value.chars() {
            let char = char as i16;
            hasher.write_i8(char as i8);
            hasher.write_i8((char >> 8) as i8);
        }
        hasher.finish() as i32
    }

    pub fn hash_boolean(value: bool) -> i32 {
        let mut hasher = Crc32cHasher::default();
        hasher.write_i8(13);
        hasher.write_i8(value as i8);
        hasher.finish() as i32
    }

    pub fn hash_int_list(values: &[i32]) -> i32 {
        let mut hasher = Crc32cHasher::default();
        hasher.write_i8(16);
        for value in values {
            hasher.write_i32(*value);
        }
        hasher.write_i8(17);
        hasher.finish() as i32
    }
}

pub struct HashOpsList {
    hasher: Crc32cHasher,
}

impl HashOpsList {
    pub fn add_raw_checksum(&mut self, checksum: i32) {
        self.hasher.write(&checksum.to_le_bytes());
    }

    pub fn add_empty(&mut self) {
        self.add_raw_checksum(HashOps::hash_empty());
    }

    pub fn add_map(&mut self, map: HashOpsMap) {
        self.add_raw_checksum(map.finish());
    }

    pub fn add_list(&mut self, list: HashOpsList) {
        self.add_raw_checksum(list.finish());
    }

    pub fn add_byte(&mut self, value: i8) {
        self.add_raw_checksum(HashOps::hash_byte(value));
    }

    pub fn add_short(&mut self, value: i16) {
        self.add_raw_checksum(HashOps::hash_short(value));
    }

    pub fn add_int(&mut self, value: i32) {
        self.add_raw_checksum(HashOps::hash_int(value));
    }

    pub fn add_long(&mut self, value: i64) {
        self.add_raw_checksum(HashOps::hash_long(value));
    }

    pub fn add_float(&mut self, value: f32) {
        self.add_raw_checksum(HashOps::hash_float(value));
    }

    pub fn add_double(&mut self, value: f64) {
        self.add_raw_checksum(HashOps::hash_double(value));
    }

    pub fn add_string(&mut self, value: &str) {
        self.add_raw_checksum(HashOps::hash_string(value));
    }

    pub fn add_boolean(&mut self, value: bool) {
        self.add_raw_checksum(HashOps::hash_boolean(value));
    }

    pub fn finish(mut self) -> i32 {
        self.hasher.write_i8(5);
        return self.hasher.finish() as i32;
    }
}

pub struct HashOpsMap {
    values: Vec<(i32, i32)>
}

impl HashOpsMap {
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn put_raw_checksum(&mut self, key: &str, value: i32) {
        self.values.push((HashOps::hash_string(key), value));
    }

    pub fn put_empty(&mut self, key: &str) {
        self.values.push((HashOps::hash_string(key), HashOps::hash_empty()));
    }

    pub fn put_list(&mut self, key: &str, list: HashOpsList) {
        self.values.push((HashOps::hash_string(key), list.finish()));
    }

    pub fn put_map(&mut self, key: &str, map: HashOpsMap) {
        self.values.push((HashOps::hash_string(key), map.finish()));
    }

    pub fn put_byte(&mut self, key: &str, value: i8) {
        self.values.push((HashOps::hash_string(key), HashOps::hash_byte(value)));
    }

    pub fn put_short(&mut self, key: &str, value: i16) {
        self.values.push((HashOps::hash_string(key), HashOps::hash_short(value)));
    }

    pub fn put_int(&mut self, key: &str, value: i32) {
        self.values.push((HashOps::hash_string(key), HashOps::hash_int(value)));
    }

    pub fn put_long(&mut self, key: &str, value: i64) {
        self.values.push((HashOps::hash_string(key), HashOps::hash_long(value)));
    }

    pub fn put_float(&mut self, key: &str, value: f32) {
        self.values.push((HashOps::hash_string(key), HashOps::hash_float(value)));
    }
    
    pub fn put_double(&mut self, key: &str, value: f64) {
        self.values.push((HashOps::hash_string(key), HashOps::hash_double(value)));
    }

    pub fn put_string(&mut self, key: &str, value: &str) {
        self.values.push((HashOps::hash_string(key), HashOps::hash_string(value)));
    }

    pub fn put_boolean(&mut self, key: &str, value: bool) {
        self.values.push((HashOps::hash_string(key), HashOps::hash_boolean(value)));
    }

    pub fn finish(mut self) -> i32 {
        let mut hasher = Crc32cHasher::default();
        hasher.write_i8(2);
        self.values.sort_by(|(k1, v1), (k2, v2)| {
            let k1 = *k1 as u64 & 4294967295;
            let k2 = *k2 as u64 & 4294967295;
            match k1.cmp(&k2) {
                std::cmp::Ordering::Less => std::cmp::Ordering::Less,
                std::cmp::Ordering::Equal => {
                    let v1 = *v1 as u64 & 4294967295;
                    let v2 = *v2 as u64 & 4294967295;
                    v1.cmp(&v2)
                },
                std::cmp::Ordering::Greater => std::cmp::Ordering::Greater,
            }
        });
        for (key, value) in self.values {
            hasher.write(&key.to_le_bytes());
            hasher.write(&value.to_le_bytes());
        }
        hasher.write_i8(3);
        return hasher.finish() as i32;
    }
}