use graphite_binary::nbt::CachedNBT;
use bytes::BufMut;

#[derive(Debug, Clone)]
pub(crate) struct BlockEntityStorage {
    dirty: bool,
    inner: Vec<BlockEntity>,

    cached_count: u32,
    cached_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct BlockEntity {
    x: usize,
    y: usize,
    z: usize,
    block_entity_type: u8,
    pub nbt: CachedNBT,
}

impl BlockEntity {
    fn calculate_key(x: usize, y: usize, z: usize) -> usize {
        x + y * 16 + z * 16 * 16
    }

    fn get_key(&self) -> usize {
        Self::calculate_key(self.x, self.y, self.z)
    }
}

impl BlockEntityStorage {
    pub fn new() -> Self {
        Self {
            dirty: false,
            inner: Vec::new(),
            cached_count: 0,
            cached_bytes: Vec::new(),
        }
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<&BlockEntity> {
        debug_assert!(x < 16);
        debug_assert!(z < 16);

        let key = BlockEntity::calculate_key(x, y, z);

        match self.inner.binary_search_by_key(&key, BlockEntity::get_key) {
            Ok(index) => Some(&self.inner[index]),
            Err(_) => None,
        }
    }

    pub fn get_or_create_mut(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        block_entity_type: u8,
    ) -> &mut BlockEntity {
        self.dirty = true;

        debug_assert!(x < 16);
        debug_assert!(z < 16);

        let key = BlockEntity::calculate_key(x, y, z);

        match self.inner.binary_search_by_key(&key, BlockEntity::get_key) {
            Ok(index) => {
                let block_entity = &mut self.inner[index];

                // Ensure the type is correct. Reset the NBT if it is not
                if block_entity.block_entity_type != block_entity_type {
                    block_entity.block_entity_type = block_entity_type;
                    let old = std::mem::replace(&mut block_entity.nbt, CachedNBT::new());
                    std::mem::drop(old);
                }

                block_entity
            }
            Err(index) => {
                self.inner.insert(
                    index,
                    BlockEntity {
                        x: x as _,
                        y: y as _,
                        z: z as _,
                        block_entity_type,
                        nbt: CachedNBT::new(),
                    },
                );
                &mut self.inner[index]
            }
        }
    }

    fn update(&mut self) {
        debug_assert!(self.dirty);
        self.dirty = false;

        self.cached_count = 0;
        self.cached_bytes.clear();

        for block_entity in &self.inner {
            self.cached_count += 1;

            debug_assert!(block_entity.x < 16);
            debug_assert!(block_entity.z < 16);

            self.cached_bytes
                .put_u8(((block_entity.x as u8) << 4) | block_entity.z as u8);
            self.cached_bytes
                .put_i16(block_entity.y as i16);
            self.cached_bytes.put_u8(block_entity.block_entity_type);
            self.cached_bytes.put_slice(block_entity.nbt.to_bytes());
        }
    }

    pub fn count(&mut self) -> usize {
        if self.dirty {
            self.update();
        }

        self.cached_count as usize
    }

    pub fn bytes(&mut self) -> &[u8] {
        if self.dirty {
            self.update();
        }

        self.cached_bytes.as_slice()
    }
}
