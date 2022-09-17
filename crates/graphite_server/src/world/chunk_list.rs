use std::slice;

use graphite_net::network_buffer::WriteBuffer;

use crate::world::{chunk_section::ChunkSection, block_entity_storage::BlockEntityStorage};

use super::chunk::Chunk;

pub struct ChunkGrid {
    size_x: usize,
    size_y: usize,
    size_z: usize,
    chunks: Vec<Chunk>
}

impl ChunkGrid {
    pub fn new(chunks: Vec<Chunk>, size_x: usize, size_y: usize, size_z: usize) -> Self {
        Self {
            size_x,
            size_y,
            size_z,
            chunks
        }
    }

    pub fn new_with_empty_chunks(size_x: usize, size_y: usize, size_z: usize) -> Self {
        let mut chunks = Vec::with_capacity(size_x * size_z);

        let base_chunk = Chunk::new_empty(size_y);
        for _ in 0..size_z*size_x {
            chunks.push(base_chunk.clone());
        }

        Self {
            size_x,
            size_y: 24,
            size_z,
            chunks
        }
    }

    pub fn new_with_default_chunks(size_x: usize, size_y: usize, size_z: usize) -> Self {
        let mut chunks = Vec::with_capacity(size_x * size_z);

        let chunk = Chunk::new_default(size_y);
        for _ in 0..size_z*size_x {
            chunks.push(chunk.clone());
        }

        Self {
            size_x,
            size_y: 24,
            size_z,
            chunks
        }
    }

    pub fn expand(&mut self, increase_x: isize, increase_y: isize, increase_z: isize) {
        let new_size_y = (self.size_y as isize + increase_y).max(1) as usize;

        if increase_y != 0 {
            for chunk in self.iter_mut() {
                chunk.expand(increase_y);
            }
            self.size_y = new_size_y;
        }

        let empty = Chunk::new_empty(new_size_y);

        if increase_x != 0 {
            // X = New value
            // _ =  Uninitialized
            // - =  Old Value
            
            // 1. (start) ------------
            // 2. (reverse exact) ------------___
            // 3. (i = 0) ---------_----X
            // 4. (i = 1) ----_----X----X
            // 5. (i = 2) ----X----X----X

            // note: a negative value of increase_x does not reduce the size,
            // instead it expands the front
            // eg. X----X----X----

            assert_eq!(self.chunks.len(), self.size_x * self.size_z);

            let abs_increase_x = increase_x.abs() as usize;
            let new_size_x = self.size_x + abs_increase_x;
            let new_chunk_count = abs_increase_x * self.size_z;
            self.chunks.reserve_exact(new_chunk_count);

            let end_offset = if increase_x > 0 { 0 } else { 1 };
            let inv_end_offset = 1 - end_offset;

            for i in (0..self.size_z).rev() {
                unsafe {
                    // Copy existing chunks from (start..start+size_x) to (end..end+size_x)
                    let start = i * self.size_x;
                    let end = (i + end_offset) * new_size_x - self.size_x * end_offset;
                    let src = self.chunks.as_mut_ptr().add(start);
                    let dst = self.chunks.as_mut_ptr().add(end);
                    std::ptr::copy(src, dst, self.size_x);

                    // Put new chunks (end..end+abs_increase_x)
                    let end = (i + inv_end_offset) * new_size_x - abs_increase_x * inv_end_offset;
                    let mut dst = self.chunks.as_mut_ptr().add(end);

                    for _ in 0..abs_increase_x {
                        std::ptr::write(dst, empty.clone());
                        dst = dst.add(1);
                    }
                }
            }

            let old_len = self.chunks.len();

            unsafe {
                self.chunks.set_len(self.chunks.len() + new_chunk_count);
            }
            self.size_x = new_size_x;

            if increase_x < 0 {
                // Move the chunk references
                for i in 0..old_len {
                    let chunk = &mut self.chunks[i + new_chunk_count];
                    let refs = chunk.pop_all_player_refs();
                    let entities = chunk.pop_all_entities();

                    let new_chunk = &mut self.chunks[i];
                    new_chunk.push_all_player_refs(refs);
                    new_chunk.push_all_entities(entities);

                }
            }
            
        }

        if increase_z != 0 {
            let abs_increase_z = increase_z.abs() as usize;
            let new_chunks = abs_increase_z * self.size_x;

            let old_len = self.chunks.len();
            
            self.chunks.reserve_exact(new_chunks);

            if increase_z > 0 {
                for _ in 0..new_chunks {
                    self.chunks.push(empty.clone());
                }
            } else {
                self.chunks.splice(0..0, std::iter::repeat(empty).take(new_chunks));

                // Move the chunk references
                for i in 0..old_len {
                    let chunk = &mut self.chunks[i + new_chunks];
                    let refs = chunk.pop_all_player_refs();
                    let entities = chunk.pop_all_entities();

                    let new_chunk = &mut self.chunks[i];
                    new_chunk.push_all_player_refs(refs);
                    new_chunk.push_all_entities(entities);
                }
            }

            self.size_z = self.size_z + increase_z.abs() as usize;
        }
    }

    pub fn get_size_x(&self) -> usize {
        self.size_x
    }

    pub fn get_size_y(&self) -> usize {
        self.size_y
    }

    pub fn get_size_z(&self) -> usize {
        self.size_z
    }

    pub fn dimensions(&self) -> (usize, usize, usize) {
        (self.size_x, self.size_y, self.size_z)
    }

    pub fn get(&self, x: usize, z: usize) -> Option<&Chunk> {
        if x >= self.size_x || z >= self.size_z {
            return None;
        }

        Some(unsafe { self.chunks.get_unchecked(x + z*self.size_x) })
    }

    pub fn get_mut(&mut self, x: usize, z: usize) -> Option<&mut Chunk> {
        if x >= self.size_x || z >= self.size_z {
            return None;
        }

        Some(unsafe { self.chunks.get_unchecked_mut(x + z*self.size_x) })
    }

    pub fn get_i32(&self, x: i32, z: i32) -> Option<&Chunk> {
        if x < 0 || z < 0 {
            return None;
        }
        self.get(x as usize, z as usize)
    }

    pub fn get_mut_i32(&mut self, x: i32, z: i32) -> Option<&mut Chunk> {
        if x < 0 || z < 0 {
            return None;
        }
        self.get_mut(x as usize, z as usize)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Chunk> {
        self.chunks.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Chunk> {
        self.chunks.iter_mut()
    }

    pub fn enumerate(&self) -> ChunkEnumerate {
        ChunkEnumerate {
            chunks: self.chunks.iter(),
            x: 0,
            z: 0,
            size_x: self.size_x
        }
    }

    pub fn enumerate_mut(&mut self) -> ChunkEnumerateMut {
        ChunkEnumerateMut {
            chunks: self.chunks.iter_mut(),
            x: 0,
            z: 0,
            size_x: self.size_x
        }
    }
}

pub struct ChunkEnumerateMut<'a> {
    chunks: slice::IterMut<'a, Chunk>,

    x: usize,
    z: usize,
    size_x: usize
}

impl<'a> Iterator for ChunkEnumerateMut<'a> {
    type Item = (usize, usize, &'a mut Chunk);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(chunk) = self.chunks.next() {
            let ret = Some((self.x, self.z, chunk));
            
            self.x += 1;
            if self.x >= self.size_x {
                self.x = 0;
                self.z += 1;
            }

            ret
        } else {
            None
        }
    }
}

pub struct ChunkEnumerate<'a> {
    chunks: slice::Iter<'a, Chunk>,

    x: usize,
    z: usize,
    size_x: usize
}

impl<'a> Iterator for ChunkEnumerate<'a> {
    type Item = (usize, usize, &'a Chunk);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(chunk) = self.chunks.next() {
            let ret = Some((self.x, self.z, chunk));

            self.x += 1;
            if self.x >= self.size_x {
                self.x = 0;
                self.z += 1;
            }

            ret
        } else {
            None
        }
    }
}