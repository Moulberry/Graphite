use graphite_binary::slice_serialization::{Single, SliceSerializable, VarInt, BigEndian};

pub const BLOCK_SIDE_LEN: usize = 16;
pub const BLOCK_ENTRY_BITS: usize = 15;

pub const BIOME_SIDE_LEN: usize = 4;
pub const BIOME_ENTRY_BITS: usize = 4; // Technically this will depend on how many biomes the server actually registers

macro_rules! define_container {
    ($data_type:ty, $side_len:expr, $array_bits:expr, $direct_bits:expr) => {
        PalettedContainer<$data_type, $side_len,
            {$side_len*$side_len*$side_len / (64 / $array_bits)}, // ARRAY_CAPACITY
            {1 << $array_bits}, // ARRAY_ITEMS
            $array_bits,
            {$side_len*$side_len*$side_len / (64 / $direct_bits)}, // DIRECT_CAPACITY
            $direct_bits
        >
    };
}

pub type BlockPalettedContainer = define_container!(u16, BLOCK_SIDE_LEN, 4, BLOCK_ENTRY_BITS);
pub type BiomePalettedContainer = define_container!(u16, BIOME_SIDE_LEN, 2, BIOME_ENTRY_BITS);

#[derive(Debug, Clone)]
pub struct ArrayContainer<T, const ARRAY_CAPACITY: usize, const ARRAY_ITEMS: usize, const ARRAY_BITS: usize> {
    pub indices: heapless::Vec<(T, usize), ARRAY_ITEMS>,
    pub contents: [u64; ARRAY_CAPACITY],
}

#[derive(Debug, Clone)]
pub struct DirectContainer<const DIRECT_CAPACITY: usize, const DIRECT_BITS: usize> {
    pub contents: [u64; DIRECT_CAPACITY],
}

#[derive(Debug, Clone)]
pub enum PalettedContainer<T, const SIDE_LEN: usize,
    const ARRAY_CAPACITY: usize, const ARRAY_ITEMS: usize, const ARRAY_BITS: usize,
    const DIRECT_CAPACITY: usize, const DIRECT_BITS: usize>
{
    Single(T),
    Array(Box<ArrayContainer<T, ARRAY_CAPACITY, ARRAY_ITEMS, ARRAY_BITS>>), // 2kb for blocks
    Direct(Box<DirectContainer<DIRECT_CAPACITY, DIRECT_BITS>>),             // 8kb for blocks
}

impl<T, const SIDE_LEN: usize,
    const ARRAY_CAPACITY: usize, const ARRAY_ITEMS: usize, const ARRAY_BITS: usize,
    const DIRECT_CAPACITY: usize, const DIRECT_BITS: usize>
    PalettedContainer<T, SIDE_LEN, ARRAY_CAPACITY, ARRAY_ITEMS, ARRAY_BITS, DIRECT_CAPACITY, DIRECT_BITS>
where
    T: Copy + Into<usize> + TryFrom<usize> + std::fmt::Debug + Eq + num::Unsigned,
{
    const VOLUME: usize = SIDE_LEN * SIDE_LEN * SIDE_LEN;
    const ARRAY_ITEMS_PER_LONG: usize = 64 / ARRAY_BITS;
    const ARRAY_ITEM_MASK: u64 = (1 << ARRAY_BITS) - 1;

    pub fn get_index(x: u8, y: u8, z: u8) -> usize {
        debug_assert!(x < SIDE_LEN as _);
        debug_assert!(y < SIDE_LEN as _);
        debug_assert!(z < SIDE_LEN as _);

        y as usize * SIDE_LEN * SIDE_LEN + z as usize * SIDE_LEN + x as usize
    }

    /// # Safety
    /// Must maintain all the invariants of ArrayContainer, namely:
    ///  - The counts of all indices must sum to capacity
    ///  - The contents must reference a value that exists in indices
    ///  - The count must match the number of times that value is referenced in contents
    ///  - Indices must not contain duplicate values
    pub unsafe fn array(indices: heapless::Vec<(T, usize), ARRAY_ITEMS>, contents: [u64; ARRAY_CAPACITY]) -> Self {
        // Check invariants in debug mode
        if cfg!(debug_assertions) {
            let mut real_counts = [0_usize; ARRAY_ITEMS];

            for index in 0..Self::VOLUME {
                let content_index = index / Self::ARRAY_ITEMS_PER_LONG;
                let shift_by = ARRAY_BITS * (index - content_index * Self::ARRAY_ITEMS_PER_LONG);
                let item = (contents[content_index] >> shift_by) & Self::ARRAY_ITEM_MASK;
                real_counts[item as usize] += 1;
            }

            // The counts of all indices must sum to capacity
            let copied = indices.clone();
            let mut sum = 0;
            for (index, (value, count)) in indices.iter().enumerate() {
                if *count == 0 {
                    // The contents must reference a value that exists in indices
                    assert_eq!(real_counts[index], 0);
                    continue;
                }

                sum += *count;

                for (index2, (value2, count2)) in copied.iter().enumerate() {
                    if index != index2 && *count2 > 0 {
                        // Indices must not contain duplicate values
                        assert_ne!(value, value2);
                    }
                }

                // The count must match the number of times that value is referenced in contents
                assert_eq!(real_counts[index], *count);
            }
            assert_eq!(sum, Self::VOLUME);
        }

        Self::Array(Box::from(ArrayContainer{ indices, contents }))
    }

    pub fn direct(contents: [u64; DIRECT_CAPACITY]) -> Self {
        Self::Direct(Box::from(DirectContainer { contents }))
    }

    pub fn filled(value: T) -> Self {
        Self::Single(value)
    }

    pub fn maybe_get_single(&self) -> Option<T> {
        match self {
            PalettedContainer::Single(value) => Some(*value),
            PalettedContainer::Array(array) => {
                if array.indices.len() == 1 {
                    Some(array.indices[0].0)
                } else {
                    None
                }
            },
            PalettedContainer::Direct(_) => {
                None
            },
        }
    }

    pub fn get(&self, x: u8, y: u8, z: u8) -> T {
        match self {
            PalettedContainer::Single(value) => *value,
            PalettedContainer::Array(array) => array.get(Self::get_index(x, y, z)),
            PalettedContainer::Direct(direct) => {
                match direct.get(Self::get_index(x, y, z)).try_into() {
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

                let mut array = Self::filled_array(value);
                array.set(Self::get_index(x, y, z), new_value);
                self.replace(Self::Array(Box::from(array)));

                Some(value)
            }
            Self::Array(array) => match array.set(Self::get_index(x, y, z), new_value) {
                ArraySetResult::Changed(old) => {
                    Some(old)
                },
                ArraySetResult::Unchanged => None,
                ArraySetResult::OutOfSpace => {
                    let mut direct = array.to_direct::<SIDE_LEN, DIRECT_CAPACITY, DIRECT_BITS>();

                    let index = Self::get_index(x, y, z);
                    let ret = direct.set(index, new_value.into()).and_then(|v| match v.try_into() {
                        Ok(v) => Some(v),
                        Err(_) => None,
                    });

                    self.replace(Self::Direct(Box::from(direct)));

                    ret
                }
            },
            Self::Direct(direct) => {
                let index = Self::get_index(x, y, z);
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
        *self = new;
    }

    fn filled_array(single: T) -> ArrayContainer<T, ARRAY_CAPACITY, ARRAY_ITEMS, ARRAY_BITS> {
        let mut indices = heapless::Vec::new();
        let _ = indices.push((single, Self::VOLUME));

        ArrayContainer {
            indices,
            contents: [0_u64; ARRAY_CAPACITY],
        }
    }

    pub fn rotate_clockwise<F>(&self, rotation: Option<F>) -> Self
        where F: Fn(T) -> T
    {
        match self {
            Self::Single(single) => {
                if let Some(rotation) = rotation {
                    Self::Single((rotation)(single.clone()))
                } else {
                    Self::Single(single.clone())
                }
            },
            Self::Array(array_container) => {
                Self::Array(Box::new(array_container.rotate_clockwise::<SIDE_LEN, _>(rotation)))
            },
            Self::Direct(direct_container) => {
                Self::Direct(Box::new(direct_container.rotate_clockwise::<SIDE_LEN, _, _>(rotation)))
            },
        }
    }

    pub fn clone_map<F>(&self, function: F) -> Self
        where F: Fn(T) -> T
    {
        match self {
            Self::Single(single) => Self::Single((function)(single.clone())),
            Self::Array(array_container) => {
                Self::Array(Box::new(array_container.clone_map::<SIDE_LEN, _>(function)))
            },
            Self::Direct(direct_container) => {
                Self::Direct(Box::new(direct_container.clone_map::<SIDE_LEN, _, _>(function)))
            },
        }
    }
}

enum ArraySetResult<T> {
    Changed(T),
    Unchanged,
    OutOfSpace,
}

impl<T, const ARRAY_CAPACITY: usize, const ARRAY_ITEMS: usize, const ARRAY_BITS: usize>
    ArrayContainer<T, ARRAY_CAPACITY, ARRAY_ITEMS, ARRAY_BITS>
where
    T: Copy + Into<usize> + Eq + std::fmt::Debug + num::Unsigned,
{
    const ARRAY_ITEMS_PER_LONG: usize = 64 / ARRAY_BITS;
    const ARRAY_ITEM_MASK: u64 = (1 << ARRAY_BITS) - 1;

    fn to_direct<const SIDE_LEN: usize, const DIRECT_CAPACITY: usize, const DIRECT_BITS: usize>(&self) -> DirectContainer<DIRECT_CAPACITY, DIRECT_BITS> {
        let mut contents = [0_u64; DIRECT_CAPACITY];

        let direct_items_per_long = 64 / DIRECT_BITS;
        
        let volume = SIDE_LEN * SIDE_LEN * SIDE_LEN;
        for index in 0..volume {
            let palette_id = self.get_palette_id(index);
            let value = self.indices[palette_id].0.into() as u64;

            let content_index = index / direct_items_per_long;
            let shift_by = DIRECT_BITS * (index - content_index * direct_items_per_long);
            contents[content_index] |= value << shift_by;
        }

        DirectContainer { contents }
    }

    fn get(&self, index: usize) -> T {
        let palette_id = self.get_palette_id(index);
        debug_assert!(self.indices[palette_id].1 > 0);
        self.indices[palette_id].0
    }

    fn set(&mut self, index: usize, new_value: T) -> ArraySetResult<T> {
        // Search for entries in the palette that match
        for (palette_index, (value, value_count)) in self.indices.iter_mut().enumerate() {
            if *value_count == 0 || *value != new_value {
                continue;
            }

            if let Some(old) = Self::set_palette_id(&mut self.contents, index, palette_index) {
                debug_assert_ne!(old as usize, palette_index);
                *value_count += 1;

                // Decrease old count
                debug_assert!(self.indices[old as usize].1 >= 1);
                self.indices[old as usize].1 -= 1;

                debug_assert_ne!(self.indices[old as usize].0, new_value);
                return ArraySetResult::Changed(self.indices[old as usize].0);
            } else {
                return ArraySetResult::Unchanged;
            }
        }

        // Search for empty entries in the palette
        for (palette_index, (value, value_count)) in self.indices.iter_mut().enumerate() {
            if *value_count != 0 {
                continue;
            }

            if let Some(old) = Self::set_palette_id(&mut self.contents, index, palette_index) {
                debug_assert_ne!(old, palette_index);
                *value_count += 1;
                *value = new_value;

                // Decrease old count
                debug_assert!(self.indices[old].1 >= 1);
                self.indices[old].1 -= 1;

                debug_assert_ne!(self.indices[old].0, new_value);
                return ArraySetResult::Changed(self.indices[old].0);
            } else {
                unreachable!("couldn't find value in palette, but when setting the value was unchanged")
            }
        }

        if self.indices.len() < self.indices.capacity() {
            let _ = self.indices.push((new_value, 1));
            if let Some(old) =
                Self::set_palette_id(&mut self.contents, index, self.indices.len() - 1)
            {
                // Decrease old count
                debug_assert!(self.indices[old as usize].1 >= 1);
                self.indices[old as usize].1 -= 1;

                debug_assert_ne!(self.indices[old as usize].0, new_value);
                ArraySetResult::Changed(self.indices[old as usize].0)
            } else {
                unreachable!("couldn't find value in palette, but when setting the value was unchanged")
            }
        } else {
            ArraySetResult::OutOfSpace
        }
    }

    fn get_palette_id(&self, index: usize) -> usize {
        let content_index = index / Self::ARRAY_ITEMS_PER_LONG;
        let shift_by = ARRAY_BITS * (index - content_index * Self::ARRAY_ITEMS_PER_LONG);
        ((self.contents[content_index] >> shift_by) & Self::ARRAY_ITEM_MASK) as usize
    }

    fn set_palette_id(
        contents: &mut [u64; ARRAY_CAPACITY],
        index: usize,
        palette_id: usize,
    ) -> Option<usize> {
        let content_index = index / Self::ARRAY_ITEMS_PER_LONG;
        let shift_by = ARRAY_BITS * (index - content_index * Self::ARRAY_ITEMS_PER_LONG);

        let mut content_value = contents[content_index];

        // Extract the old value
        let old_value = ((content_value >> shift_by) & Self::ARRAY_ITEM_MASK) as usize;
        if old_value == palette_id {
            return None;
        }

        // Update content_value to contain the new value
        content_value &= !(Self::ARRAY_ITEM_MASK << shift_by);
        content_value |= (palette_id as u64) << shift_by;
        contents[content_index] = content_value;

        Some(old_value)
    }

    pub fn rotate_clockwise<const SIDE_LEN: usize, F>(&self, rotation: Option<F>) -> Self
        where F: Fn(T) -> T
    {
        let mut contents = [0_u64; ARRAY_CAPACITY];

        let volume = SIDE_LEN * SIDE_LEN * SIDE_LEN;
        for index in 0..volume {
            let palette_id = self.get_palette_id(index);

            let x = index % SIDE_LEN;
            let y = index / SIDE_LEN / SIDE_LEN;
            let z = (index / SIDE_LEN) % SIDE_LEN;
            let new_index = y * SIDE_LEN * SIDE_LEN + x * SIDE_LEN + (SIDE_LEN - 1 - z);

            let content_index = new_index / Self::ARRAY_ITEMS_PER_LONG;
            let shift_by = ARRAY_BITS * (new_index - content_index * Self::ARRAY_ITEMS_PER_LONG);

            contents[content_index] |= (palette_id as u64) << shift_by;
        }

        let mut indices;
        if let Some(rotation) = rotation {
            indices = heapless::Vec::new();
            for palette in &self.indices {
                let rotated = (rotation)(palette.0.clone());
                let _ = indices.push((rotated, palette.1));
            }
        } else {
            indices = self.indices.clone();
        };

        Self {
            indices,
            contents,
        }
    }

    pub fn clone_map<const SIDE_LEN: usize, F>(&self, function: F) -> Self
        where F: Fn(T) -> T
    {

        let mut remap = Vec::new();

        let mut new_indices = heapless::Vec::new();
        'indices: for (index, palette) in self.indices.iter().enumerate() {
            let new_value = (function)(palette.0.clone());

            for (new_index, (existing_value, existing_count)) in new_indices.iter_mut().enumerate() {
                if *existing_value == new_value {
                    *existing_count += palette.1;
                    remap.push((index, new_index));
                    continue 'indices;
                }
            }

            if new_indices.len() != index {
                remap.push((index, new_indices.len()));
            }
            let _ = new_indices.push((new_value, palette.1));
        }

        let mut contents;

        if remap.is_empty() {
            contents = self.contents.clone();
        } else {
            contents = [0_u64; ARRAY_CAPACITY];

            let volume = SIDE_LEN * SIDE_LEN * SIDE_LEN;
            for index in 0..volume {
                let mut palette_id = self.get_palette_id(index);

                let content_index = index / Self::ARRAY_ITEMS_PER_LONG;
                let shift_by = ARRAY_BITS * (index - content_index * Self::ARRAY_ITEMS_PER_LONG);

                for (from, to) in remap.iter() {
                    if palette_id == *from {
                        palette_id = *to;
                        break;
                    }
                }

                contents[content_index] |= (palette_id as u64) << shift_by;
            }
        }
        

        Self {
            indices: new_indices,
            contents,
        }
    }
}

impl<const DIRECT_CAPACITY: usize, const DIRECT_BITS: usize> DirectContainer<DIRECT_CAPACITY, DIRECT_BITS> {
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
        let value = (content_value >> shift_by) & mask;

        value as usize
    }

    pub fn rotate_clockwise<const SIDE_LEN: usize, T, F>(&self, rotation: Option<F>) -> Self
        where F: Fn(T) -> T, T: Copy + Into<usize> + TryFrom<usize> + std::fmt::Debug + Eq + num::Unsigned,
    {
        let mut contents = [0_u64; DIRECT_CAPACITY];

        let volume = SIDE_LEN * SIDE_LEN * SIDE_LEN;

        if let Some(rotation) = rotation {
            let mut last_value = usize::MAX;
            let mut last_mapped_value = 0;

            for index in 0..volume {
                let value = self.get(index);
    
                let x = index % SIDE_LEN;
                let y = index / SIDE_LEN / SIDE_LEN;
                let z = (index / SIDE_LEN) % SIDE_LEN;
                let new_index = y * SIDE_LEN * SIDE_LEN + x * SIDE_LEN + (SIDE_LEN - 1 - z);
    
                if value != last_value {
                    let value_t = T::try_from(value).ok().unwrap_or(T::zero());
                    let new_value_t = (rotation)(value_t);
                    let new_value: usize = new_value_t.into();
    
                    last_value = value;
                    last_mapped_value = new_value;
                }
    
                let per_array = 64 / DIRECT_BITS;
                let content_index = new_index / per_array;
                let shift_by = DIRECT_BITS * (new_index % per_array);
    
                contents[content_index] |= (last_mapped_value as u64) << shift_by;
            }
        } else {
            for index in 0..volume {
                let value = self.get(index);

                let x = index % SIDE_LEN;
                let y = index / SIDE_LEN / SIDE_LEN;
                let z = (index / SIDE_LEN) % SIDE_LEN;
                let new_index = y * SIDE_LEN * SIDE_LEN + x * SIDE_LEN + (SIDE_LEN - 1 - z);

                let per_array = 64 / DIRECT_BITS;
                let content_index = new_index / per_array;
                let shift_by = DIRECT_BITS * (new_index % per_array);

                contents[content_index] |= (value as u64) << shift_by;
            }
        };

        Self {
            contents,
        }
    }

    pub fn clone_map<const SIDE_LEN: usize, T, F>(&self, function: F) -> Self
        where F: Fn(T) -> T, T: Copy + Into<usize> + TryFrom<usize> + std::fmt::Debug + Eq + num::Unsigned,
    {
        let mut contents = [0_u64; DIRECT_CAPACITY];

        let mut last_value = usize::MAX;
        let mut last_mapped_value = 0;

        let volume = SIDE_LEN * SIDE_LEN * SIDE_LEN;
        for index in 0..volume {
            let value = self.get(index);

            if value != last_value {
                let value_t = T::try_from(value).ok().unwrap_or(T::zero());
                let new_value_t = (function)(value_t);
                let new_value: usize = new_value_t.into();

                last_value = value;
                last_mapped_value = new_value;
            }

            let per_array = 64 / DIRECT_BITS;
            let content_index = index / per_array;
            let shift_by = DIRECT_BITS * (index % per_array);

            contents[content_index] |= (last_mapped_value as u64) << shift_by;
        }

        Self {
            contents,
        }
    }
}

impl<'r, 'd: 'r, T: 'static,
    const SIDE_LEN: usize,
    const ARRAY_CAPACITY: usize, const ARRAY_ITEMS: usize, const ARRAY_BITS: usize,
    const DIRECT_CAPACITY: usize, const DIRECT_BITS: usize>
    SliceSerializable<'r, 'd> for PalettedContainer<T, SIDE_LEN, ARRAY_CAPACITY, ARRAY_ITEMS, ARRAY_BITS, DIRECT_CAPACITY, DIRECT_BITS>
where
    T: Copy + Into<i32> + std::fmt::Debug,
{
    type CopyType = &'r Self;

    fn as_copy_type(t: &'r Self) -> Self::CopyType {
        t
    }

    fn read(_: &mut &[u8]) -> anyhow::Result<Self> {
        unimplemented!()
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], data: &'r Self) -> &'b mut [u8] {
        match data {
            Self::Single(value) => {
                debug_assert!(
                    bytes.len() >= 5,
                    "invariant: slice must contain at least 5 bytes to write paletted_container (single)"
                );
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0); // 0 bits per block
                <VarInt as SliceSerializable<i32>>::write(bytes, (*value).into()) // the block
                // <Single as SliceSerializable<u8>>::write(bytes, 0) // 0 size array
            }
            Self::Array(array) => {
                debug_assert!(
                    bytes.len() >= 7 + 5*array.indices.len() + ARRAY_CAPACITY*8,
                    "invariant: slice must contain at least 7+5*array.indices.len()+ARRAY_CAPACITY*8 bytes to write paletted_container (array)"
                );

                bytes = <Single as SliceSerializable<u8>>::write(bytes, ARRAY_BITS as u8);

                // palette
                bytes = <Single as SliceSerializable<u8>>::write(bytes, array.indices.len() as _); // palette length
                for (entry, _) in &array.indices {
                    bytes = <VarInt as SliceSerializable<i32>>::write(bytes, (*entry).into()); // the palette entry
                }

                // data
                // bytes = <VarInt as SliceSerializable<i32>>::write(bytes, ARRAY_CAPACITY as i32);

                // todo: is there a more efficient way of doing this?
                for value in &array.contents {
                    bytes = <BigEndian as SliceSerializable<u64>>::write(bytes, *value);
                }
                bytes
            }
            Self::Direct(direct) => {
                debug_assert!(
                    bytes.len() >= 1 + 5 + DIRECT_CAPACITY*8,
                    "invariant: slice must contain at least 6+DIRECT_LEN*8 bytes to write paletted_container (direct)"
                );

                bytes = <Single as SliceSerializable<u8>>::write(bytes, DIRECT_BITS as u8);
                // bytes = <VarInt as SliceSerializable<i32>>::write(bytes, DIRECT_CAPACITY as i32);

                // todo: is there a more efficient way of doing this?
                for value in &direct.contents {
                    bytes = <BigEndian as SliceSerializable<u64>>::write(bytes, *value);
                }

                bytes
            }
        }

    }

    fn get_write_size(data: &'r Self) -> usize {
        let size = 1 + /*bits-per-block*/ match data {
            Self::Single(_) => 5 /*blockstate*/,
            Self::Array(array) => {
                1+5*array.indices.len() /*palette*/ +
                ARRAY_CAPACITY*8 /*contents*/ },
            Self::Direct(_) => DIRECT_CAPACITY*8 /*contents*/,
        };
        size
    }
}