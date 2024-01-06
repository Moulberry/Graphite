use graphite_binary::slice_serialization::{Single, SliceSerializable, VarInt, BigEndian};

pub const BLOCK_SIDE_LEN: usize = 16;
pub const BLOCK_CAPACITY: usize = BLOCK_SIDE_LEN * BLOCK_SIDE_LEN * BLOCK_SIDE_LEN;
pub const BLOCK_ENTRY_BITS: usize = 15;
pub const BLOCK_DIRECT_LEN: usize = BLOCK_CAPACITY / (64 / BLOCK_ENTRY_BITS);

pub const BIOME_SIDE_LEN: usize = 4;
pub const BIOME_CAPACITY: usize = BIOME_SIDE_LEN * BIOME_SIDE_LEN * BIOME_SIDE_LEN;
pub const BIOME_ENTRY_BITS: usize = 4;
pub const BIOME_DIRECT_LEN: usize = BIOME_CAPACITY / (64 / BIOME_ENTRY_BITS);

pub type BlockPalettedContainer =
    PalettedContainer<u16, BLOCK_SIDE_LEN, { BLOCK_CAPACITY / 2 }, BLOCK_DIRECT_LEN, BLOCK_ENTRY_BITS>;
pub type BiomePalettedContainer =
    PalettedContainer<u8, BIOME_SIDE_LEN, { BIOME_CAPACITY / 2 }, BIOME_DIRECT_LEN, BIOME_ENTRY_BITS>;

#[derive(Debug, Clone)]
pub struct ArrayContainer<T, const HALF_CAP: usize> {
    pub indices: heapless::Vec<(T, usize), 16>,
    pub contents: [u8; HALF_CAP],
}

#[derive(Debug, Clone)]
pub struct DirectContainer<const DIRECT_LEN: usize, const DIRECT_BITS: usize> {
    pub contents: [u64; DIRECT_LEN],
}

#[derive(Debug, Clone)]
pub enum PalettedContainer<T, const SIDE_LEN: usize, const HALF_CAP: usize, const DIRECT_LEN: usize, const DIRECT_BITS: usize>
{
    Single(T),
    Array(Box<ArrayContainer<T, HALF_CAP>>),                // 2kb
    Direct(Box<DirectContainer<DIRECT_LEN, DIRECT_BITS>>),  // 8kb
}

impl<T, const SIDE_LEN: usize, const HALF_CAP: usize, const DIRECT_LEN: usize, const DIRECT_BITS: usize>
    PalettedContainer<T, SIDE_LEN, HALF_CAP, DIRECT_LEN, DIRECT_BITS>
where
    T: Copy + Into<usize> + TryFrom<usize> + Eq + num::Unsigned,
{
    pub fn get_array_index(x: u8, y: u8, z: u8) -> usize {
        debug_assert!(x < SIDE_LEN as _);
        debug_assert!(y < SIDE_LEN as _);
        debug_assert!(z < SIDE_LEN as _);

        y as usize * SIDE_LEN * SIDE_LEN + z as usize * SIDE_LEN + (15 - x) as usize
        // 15 - x because minecraft expects the data in big endian form
    }

    /// # Safety
    /// Must maintain all the invariants of ArrayContainer, namely:
    ///  - The counts of all indices must sum to 4096
    ///  - The contents must reference a value that exists in indices
    ///  - The count must match the number of times that value is referenced in contents
    pub unsafe fn array(indices: heapless::Vec<(T, usize), 16>, contents: [u8; HALF_CAP]) -> Self {
        Self::Array(Box::from(ArrayContainer{ indices, contents }))
    }

    pub fn direct(contents: [u64; DIRECT_LEN]) -> Self {
        Self::Direct(Box::from(DirectContainer { contents }))
    }

    pub fn filled(value: T) -> Self {
        Self::Single(value)
    }

    pub fn get(&self, x: u8, y: u8, z: u8) -> T {
        match self {
            PalettedContainer::Single(value) => *value,
            PalettedContainer::Array(array) => array.get(Self::get_array_index(x, y, z)),
            PalettedContainer::Direct(direct) => {
                let index = y as usize * SIDE_LEN * SIDE_LEN + z as usize * SIDE_LEN + x as usize;
                match direct.get(index).try_into() {
                    Ok(v) => v,
                    Err(_) => T::zero(),
                }
            },
        }
    }

    pub fn set(&mut self, x: u8, y: u8, z: u8, new_value: T) -> Option<T> {
        match self {
            Self::Single(value) => {
                let value = *value;
                if value == new_value {
                    return None;
                }

                let mut array = Self::filled_array(value, SIDE_LEN * SIDE_LEN * SIDE_LEN);
                array.set(Self::get_array_index(x, y, z), new_value);
                self.replace(Self::Array(Box::from(array)));

                Some(value)
            }
            Self::Array(array) => match array.set(Self::get_array_index(x, y, z), new_value) {
                ArraySetResult::Changed(old) => Some(old),
                ArraySetResult::Unchanged => None,
                ArraySetResult::OutOfSpace => {
                    let mut direct = array.to_direct::<DIRECT_LEN, DIRECT_BITS>();
                    
                    let index = y as usize * SIDE_LEN * SIDE_LEN + z as usize * SIDE_LEN + x as usize;
                    let ret = direct.set(index, new_value.into()).and_then(|v| match v.try_into() {
                        Ok(v) => Some(v),
                        Err(_) => None,
                    });

                    self.replace(Self::Direct(Box::from(direct)));

                    ret
                }
            },
            Self::Direct(direct) => {
                let index = y as usize * SIDE_LEN * SIDE_LEN + z as usize * SIDE_LEN + x as usize;
                direct.set(index, new_value.into()).and_then(|v| match v.try_into() {
                    Ok(v) => Some(v),
                    Err(_) => None,
                })
            },
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

    fn filled_array(single: T, count: usize) -> ArrayContainer<T, HALF_CAP> {
        let mut indices = heapless::Vec::new();
        let _ = indices.push((single, count));

        ArrayContainer {
            indices,
            contents: [0_u8; HALF_CAP],
        }
    }
}

enum ArraySetResult<T> {
    Changed(T),
    Unchanged,
    OutOfSpace,
}

impl<T, const HALF_CAP: usize> ArrayContainer<T, HALF_CAP>
where
    T: Copy + Into<usize> + Eq + num::Unsigned,
{
    fn to_direct<const DIRECT_LEN: usize, const DIRECT_BITS: usize>(&self) -> DirectContainer<DIRECT_LEN, DIRECT_BITS> {
        let mut contents = [0_u64; DIRECT_LEN];

        let mut content_index = 0;
        let mut shift = 0;
        
        for i in 0..HALF_CAP/8 {
            for j in 0..8 {
                // The order of the entries need to be reversed
                // We store little-endian, but the protocol wants big-endian
                let array_index = i*8 + 7-j;
                let v = self.contents[array_index];

                let first = v & 0b1111;
                let (first_value, _) = self.indices[first as usize];
                let first_value = first_value.into() as u64;

                // Make sure the value fits inside DIRECT_BITS
                debug_assert!(first_value.leading_zeros() >= (64 - DIRECT_BITS) as u32);

                contents[content_index] |= first_value << shift;

                // Increment
                shift += DIRECT_BITS;
                if shift + DIRECT_BITS > 64 {
                    content_index += 1;
                    shift = 0;
                }

                let second = v >> 4;
                let (second_value, _) = self.indices[second as usize];
                let second_value = second_value.into() as u64;

                // Make sure the value fits inside DIRECT_BITS
                debug_assert!(second_value.leading_zeros() >= (64 - DIRECT_BITS) as u32);

                contents[content_index] |= second_value << shift;

                // Increment
                shift += DIRECT_BITS;
                if shift + DIRECT_BITS > 64 {
                    content_index += 1;
                    shift = 0;
                }
            }
        }

        DirectContainer { contents }
    }

    fn get(&self, index: usize) -> T {
        let palette_id = self.get_as_palette(index);
        self.indices[palette_id as usize].0
    }

    fn set(&mut self, index: usize, new_value: T) -> ArraySetResult<T> {
        for (palette_index, (value, value_count)) in self.indices.iter_mut().enumerate() {
            if *value_count == 0 {
                *value = new_value;
            } else if *value != new_value {
                continue;
            }

            if let Some(old) = Self::set_to_palette(&mut self.contents, index, palette_index) {
                debug_assert_ne!(old as usize, palette_index);
                *value_count += 1;

                // Decrease old count
                debug_assert!(self.indices[old as usize].1 >= 1);
                self.indices[old as usize].1 -= 1;

                return ArraySetResult::Changed(self.indices[old as usize].0);
            } else {
                return ArraySetResult::Unchanged;
            }
        }

        if self.indices.len() < self.indices.capacity() {
            let _ = self.indices.push((new_value, 1));
            if let Some(old) =
                Self::set_to_palette(&mut self.contents, index, self.indices.len() - 1)
            {
                // Decrease old count
                debug_assert!(self.indices[old as usize].1 >= 1);
                self.indices[old as usize].1 -= 1;

                ArraySetResult::Changed(self.indices[old as usize].0)
            } else {
                unreachable!("couldn't find value in palette, but when setting the value was unchanged")
            }
        } else {
            ArraySetResult::OutOfSpace
        }
    }

    fn get_as_palette(&self, content_index: usize) -> u8 {
        let nibble_pair = self.contents[content_index / 2];
        if content_index % 2 == 0 {
            (nibble_pair & 0b11110000) >> 4
        } else {
            nibble_pair & 0b00001111
        }
    }

    fn set_to_palette(
        contents: &mut [u8; HALF_CAP],
        content_index: usize,
        palette_index: usize,
    ) -> Option<u8> {
        let nibble_pair = contents[content_index / 2];

        let offset = ((content_index + 1) % 2) * 4;
        let mask = 0b1111 << offset;

        let new_nibble_pair = nibble_pair & (!mask) | ((palette_index as u8) << offset);

        if new_nibble_pair != nibble_pair {
            contents[content_index / 2] = new_nibble_pair;
            Some((nibble_pair & mask) >> offset)
        } else {
            None
        }
    }
}

impl<const DIRECT_LEN: usize, const DIRECT_BITS: usize> DirectContainer<DIRECT_LEN, DIRECT_BITS> {
    fn set(&mut self, index: usize, new_value: usize) -> Option<usize> {
        let per_array = 64 / DIRECT_BITS;
        let content_index = index / per_array;
        let shift_by = DIRECT_BITS * (index % per_array);
        let mask = (1 << DIRECT_BITS) - 1;

        let new_value = new_value as u64;

        // Make sure the value fits inside DIRECT_BITS
        debug_assert!(new_value.leading_zeros() >= (64 - DIRECT_BITS) as u32);

        let mut content_value = self.contents[content_index];

        // Extract the old value
        let old_value = (content_value >> shift_by) & mask;
        if old_value == new_value {
            return None;
        }

        // Update content_value to contain the new value
        content_value &= !(mask << shift_by);
        content_value |= (new_value as u64) << shift_by;
        self.contents[content_index] = content_value;

        Some(old_value as usize)
    }

    fn get(&self, index: usize) -> usize {
        let per_array = 64 / DIRECT_BITS;
        let content_index = index / per_array;
        let shift_by = DIRECT_BITS * (index % per_array);
        let mask = (1 << DIRECT_BITS) - 1;

        let content_value = self.contents[content_index];
        let old_value = (content_value >> shift_by) & mask;

        old_value as usize
    }
}

impl<'a, T: 'static, const SIDE_LEN: usize, const HALF_CAP: usize, const DIRECT_LEN: usize, const DIRECT_BITS: usize>
    SliceSerializable<'a> for PalettedContainer<T, SIDE_LEN, HALF_CAP, DIRECT_LEN, DIRECT_BITS>
where
    T: Copy + Into<i32> + std::fmt::Debug,
{
    type CopyType = &'a Self;

    fn as_copy_type(t: &'a Self) -> Self::CopyType {
        t
    }

    fn read(_: &mut &[u8]) -> anyhow::Result<Self> {
        unimplemented!()
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], data: &'a Self) -> &'b mut [u8] {
        match data {
            Self::Single(value) => {
                debug_assert!(
                    bytes.len() >= 5,
                    "invariant: slice must contain at least 5 bytes to write paletted_container (single)"
                );
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0); // 0 bits per block
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, (*value).into()); // the block
                <Single as SliceSerializable<u8>>::write(bytes, 0) // 0 size array
            }
            Self::Array(array) => {
                debug_assert!(
                    bytes.len() >= 7 + 3*array.indices.len() + HALF_CAP,
                    "invariant: slice must contain at least 7+3*array.indices.len()+HALF_CAP bytes to write paletted_container (array)"
                );

                bytes = <Single as SliceSerializable<u8>>::write(bytes, 4); // 4 bits per block

                // palette
                bytes = <Single as SliceSerializable<u8>>::write(bytes, array.indices.len() as _); // palette length
                for (entry, _) in &array.indices {
                    bytes = <VarInt as SliceSerializable<i32>>::write(bytes, (*entry).into()); // the palette entry
                }

                // data
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, (HALF_CAP / 8) as i32);
                bytes[..HALF_CAP].clone_from_slice(&array.contents);
                &mut bytes[HALF_CAP..]
            }
            Self::Direct(direct) => {
                debug_assert!(
                    bytes.len() >= 1 + 5 + DIRECT_LEN*8,
                    "invariant: slice must contain at least 6+DIRECT_LEN*8 bytes to write paletted_container (direct)"
                );

                bytes = <Single as SliceSerializable<u8>>::write(bytes, 15); // 15 bits per block

                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, DIRECT_LEN as i32);

                // todo: is there a more efficient way of doing this?
                for value in &direct.contents {
                    bytes = <BigEndian as SliceSerializable<u64>>::write(bytes, *value);
                }

                bytes
            }
        }

    }

    fn get_write_size(data: &'a Self) -> usize {
        let size = 1 + /*bits-per-block*/ match data {
            Self::Single(_) => 3 /*blockstate*/ + 1 /*empty array header*/,
            Self::Array(array) => {
                1+3*array.indices.len() /*palette*/ +
                5 /*array header*/ + HALF_CAP /*contents*/ },
            Self::Direct(_) => 5 /*array header*/ + DIRECT_LEN*8 /*contents*/,
        };
        size
    }
}