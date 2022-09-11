use std::slice;

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

    pub fn new_with_default_chunks(size_x: usize, size_y: usize, size_z: usize) -> Self {
        let mut chunks = Vec::with_capacity(size_x * size_z);

        for z in 0..size_z {
            for x in 0..size_x {
                chunks.push(Chunk::new_with_default_chunks(false, size_y, x, z));
            }
        }

        Self {
            size_x,
            size_y: 24,
            size_z,
            chunks
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