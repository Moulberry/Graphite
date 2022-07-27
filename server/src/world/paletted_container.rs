use binary::slice_serialization::{Single, SliceSerializable, VarInt};

const BLOCK_SIDE_LEN: usize = 16;
const BLOCK_CAPACITY: usize = BLOCK_SIDE_LEN * BLOCK_SIDE_LEN * BLOCK_SIDE_LEN;
const BLOCK_ENTRY_BITS: usize = 15;
const BLOCK_DIRECT_LEN: usize = BLOCK_CAPACITY / (64 / BLOCK_ENTRY_BITS);

const BIOME_SIDE_LEN: usize = 4;
const BIOME_CAPACITY: usize = BIOME_SIDE_LEN * BIOME_SIDE_LEN * BIOME_SIDE_LEN;
const BIOME_ENTRY_BITS: usize = 4;
const BIOME_DIRECT_LEN: usize = BIOME_CAPACITY / (64 / BIOME_ENTRY_BITS);

pub type BlockPalettedContainer = PalettedContainer<u16, BLOCK_SIDE_LEN, { BLOCK_CAPACITY / 2 }, BLOCK_DIRECT_LEN>;
pub type BiomePalettedContainer = PalettedContainer<u8, BIOME_SIDE_LEN, { BIOME_CAPACITY / 2 }, BIOME_DIRECT_LEN>;

#[derive(Debug, Clone)]
pub struct ArrayContainer<T, const HALF_CAP: usize> {
    indices: heapless::Vec<T, 16>,
    contents: [u8; HALF_CAP],
}

#[allow(warnings)] // todo: when DirectContainer is used, this must be removed
#[derive(Debug, Clone)]
pub struct DirectContainer<T, const DIRECT_LEN: usize> {
    most_common_type: T,
    most_common_count: u16,
    contents: [u64; DIRECT_LEN],
}

#[derive(Debug, Clone)]
pub enum PalettedContainer<T, const SIDE_LEN: usize, const HALF_CAP: usize, const DIRECT_LEN: usize> {
    Single(T),
    Array(Box<ArrayContainer<T, HALF_CAP>>),     // 2kb
    Direct(Box<DirectContainer<T, DIRECT_LEN>>), // 8kb
}

impl<T, const SIDE_LEN: usize, const HALF_CAP: usize, const DIRECT_LEN: usize> PalettedContainer<T, SIDE_LEN, HALF_CAP, DIRECT_LEN>
where
    T: Copy + Eq + num::Unsigned
{
    pub fn get_index(x: u8, y: u8, z: u8) -> usize {
        debug_assert!(x < SIDE_LEN as _);
        debug_assert!(y < SIDE_LEN as _);
        debug_assert!(z < SIDE_LEN as _);

        y as usize * SIDE_LEN * SIDE_LEN +
        z as usize * SIDE_LEN +
        (15 - x) as usize // 15 - x because minecraft expects the data in big endian form
    }

    pub fn filled(value: T) -> Self {
        Self::Single(value)
    }

    pub fn set(&mut self, x: u8, y: u8, z: u8, new_value: T) -> bool {
        match self {
            Self::Single(value) => {
                if *value == new_value {
                    println!("no changes have been made!");
                    return false;
                }

                let mut array = Self::filled_array(*value);
                array.set_new(Self::get_index(x, y, z), new_value);
                self.replace(Self::Array(Box::from(array)));

                true
            },
            Self::Array(array) => {
                match array.set(Self::get_index(x, y, z), new_value) {
                    ArraySetResult::Changed => return true,
                    ArraySetResult::Unchanged => return false,
                    ArraySetResult::OutOfSpace => {
                        todo!("switch to direct");
                    },
                }
            },
            Self::Direct(_) => todo!(),
        }
    }

    pub fn fill(&mut self, new_value: T) -> bool {
        if let Self::Single(value) = self {
            if *value == new_value {
                return false;
            }
        }
        self.replace(Self::Single(new_value));
        true
    }

    fn replace(&mut self, new: Self) {
        unsafe {
            std::ptr::write(self, new);
        }
    }

    fn filled_array(single: T) -> ArrayContainer<T, HALF_CAP> {
        let mut indices = heapless::Vec::new();
        let _ = indices.push(single);

        ArrayContainer {
            indices,
            contents: [0_u8; HALF_CAP],
        }
    }
}

enum ArraySetResult {
    Changed,
    Unchanged,
    OutOfSpace
}

impl<T, const HALF_CAP: usize> ArrayContainer<T, HALF_CAP>
where
    T: Copy + Eq + num::Unsigned
{
    fn set(&mut self, index: usize, new_value: T) -> ArraySetResult {
        for (palette_index, value) in self.indices.iter().enumerate() {
            if *value == new_value {
                if self.set_to_palette(index, palette_index) {
                    return ArraySetResult::Changed;
                } else {
                    return ArraySetResult::Unchanged;
                }
            }
        }

        if self.indices.len() < self.indices.capacity() {
            let _ = self.indices.push(new_value);
            self.set_to_palette(index, self.indices.len() - 1);
            ArraySetResult::Changed
        } else {
            ArraySetResult::OutOfSpace
        }
    }

    fn set_new(&mut self, index: usize, new_value: T) {
        if self.indices.len() < self.indices.capacity() {
            let _ = self.indices.push(new_value);
            self.set_to_palette(index, self.indices.len() - 1);
        } else {
            panic!("Not enough room in palette");
        }
    }

    fn set_to_palette(&mut self, content_index: usize, palette_index: usize) -> bool {
        let nibble_pair = self.contents[content_index/2];

        let offset = ((content_index + 1) % 2) * 4;
        let mask = 0b1111 << offset;

        let new_nibble_pair = nibble_pair & (!mask) | ((palette_index as u8) << offset);

        if new_nibble_pair != nibble_pair {
            self.contents[content_index/2] = new_nibble_pair;
            true
        } else {
            false
        }
    }
}

impl<'a, T: 'static, const SIDE_LEN: usize, const HALF_CAP: usize, const DIRECT_LEN: usize> SliceSerializable<'a>
    for PalettedContainer<T, SIDE_LEN, HALF_CAP, DIRECT_LEN>
where
    T: Copy + Into<i32>,
{
    type RefType = &'a Self;

    fn maybe_deref(t: &'a Self) -> Self::RefType {
        t
    }

    fn read(_: &mut &[u8]) -> anyhow::Result<Self> {
        unimplemented!()
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], data: &'a Self) -> &'b mut [u8] {
        match data {
            Self::Single(value) => {
                bytes = <Single as SliceSerializable<'_, u8>>::write(bytes, 0); // 0 bits per block
                bytes = VarInt::write(bytes, (*value).into()); // the block
                <Single as SliceSerializable<'_, u8>>::write(bytes, 0) // 0 size array
            }
            Self::Array(array) => {
                bytes = <Single as SliceSerializable<'_, u8>>::write(bytes, 4); // 4 bits per block

                // palette
                bytes = <Single as SliceSerializable<'_, u8>>::write(bytes, array.indices.len() as _); // palette length
                for entry in &array.indices {
                    bytes = VarInt::write(bytes, (*entry).into()); // the palette entry
                }

                // data
                bytes = VarInt::write(bytes, (HALF_CAP/8) as i32);
                bytes[..HALF_CAP].clone_from_slice(&array.contents);
                &mut bytes[HALF_CAP..]
            }
            Self::Direct(_direct) => {
                todo!();
            }
            /*Self::Direct { data } => {
                bytes = <Single as SliceSerializable<'_, u8>>::write(bytes, 15); // 15 bits per block

                bytes = VarInt::write(bytes, 1024 as i32);
                bytes[..8192].clone_from_slice(std::mem::transmute::<&[u64; 1024], &[u8; 8192]>(&data));
                &mut bytes[8192..]
            }*/
        }
    }

    fn get_write_size(data: &'a Self) -> usize {
        1 + /*bits-per-block*/ match data {
            Self::Single(_) => 3 /*blockstate*/ + 1 /*empty array header*/,
            Self::Array(_) => 1+std::mem::size_of::<T>()*16 /*palette*/ + HALF_CAP /*contents*/ + 1 /*empty array header*/,
            Self::Direct(_) => 2 /*array header*/ + DIRECT_LEN*8 /*conents*/,
        }
    }
}

/*impl PalettedContainer {
    pub fn fill(&mut self, new_value: u16) -> bool {
        match self {
            PalettedContainer::Single { value } => {
                if *value == new_value {
                    return false;
                }
            },
            _ => {}
        }

        // Replace with single
        self.replace(Self::Single {
            value: new_value
        });

        true
    }

    pub fn set(&mut self, x: u8, y: u8, z: u8, new_value: u16) -> bool {
        match self {
            Self::Single { value } => {
                if *value == new_value {
                    return false;
                }
                let direct = Self::convert_single_to_direct(*value);
                self.replace(direct);
            },
            Self::Direct { data } => {
                Self::set_direct(x, y, z, new_value, data.as_slice());
            },
        }
        true
    }

    fn replace(&mut self, new: Self) {
        unsafe {
            std::ptr::write(self, new);
        }
    }

    fn set_direct(x: u8, y: u8, z: u8, new_value: u16, data: &[u64]) {

    }

    fn convert_single_to_direct(single: u16) -> Self {
        let single = single as u64;
        let filled_value = single | (single << 15) | (single << 30) | (single << 45);

        Self::Direct {
            data: Box::from([filled_value; 1024])
        }
    }
}

impl<'a> SliceSerializable<'a> for PalettedContainer {
    type RefType = &'a Self;

    fn maybe_deref(t: &'a Self) -> Self::RefType {
        t
    }

    fn read(_: &mut &[u8]) -> anyhow::Result<Self> {
        unimplemented!()
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], data: &'a Self) -> &'b mut [u8] {
        match data {
            Self::Single { value } => {
                bytes = <Single as SliceSerializable<'_, u8>>::write(bytes, 0); // 0 bits per block
                bytes = slice_serialization::VarInt::write(bytes, *value as i32); // the block
                <Single as SliceSerializable<'_, u8>>::write(bytes, 0) // 0 size array
            }
            Self::Direct { data } => {
                bytes = <Single as SliceSerializable<'_, u8>>::write(bytes, 15); // 15 bits per block

                bytes = VarInt::write(bytes, 1024 as i32);
                bytes[..8192].clone_from_slice(std::mem::transmute::<&[u64; 1024], &[u8; 8192]>(&data));
                &mut bytes[8192..]
            }
        }
    }

    fn get_write_size(data: &'a Self) -> usize {
        1 + match data {
            Self::Single { value: _ } => 5 + 1, // varint (block) + array header
            Self::Direct { data: _ } => 5 + 8192, // varint array header + data
        }
    }
}
*/
