use glam::DVec2;

use super::chunk::Chunk;
pub struct NearbyChunkIter<'a> {
    chunks: &'a [Chunk],
    min_chunk: (i32, i32),
    max_chunk: (i32, i32),
    current_chunk: (i32, i32),
    chunk_stride: i32,

    position: DVec2,
    distance_sq: f64,
    empty: bool
}

impl <'a> NearbyChunkIter <'a> {
    pub fn new(chunks: &'a [Chunk], chunks_x: i32, chunks_z: i32, position: DVec2, distance: f64) -> Self {
        let min_chunk_x = (position.x - distance).floor() as i32 >> 4;
        let min_chunk_z = (position.y - distance).floor() as i32 >> 4;
        let max_chunk_x = (position.x + distance).floor() as i32 >> 4;
        let max_chunk_z = (position.y + distance).floor() as i32 >> 4;

        let min_chunk_x = min_chunk_x.max(0);
        let min_chunk_z = min_chunk_z.max(0);
        let max_chunk_x = max_chunk_x.min(chunks_x - 1);
        let max_chunk_z = max_chunk_z.min(chunks_z - 1);

        let empty = min_chunk_x > max_chunk_x || min_chunk_z > max_chunk_z;

        return Self {
            chunks,
            min_chunk: (min_chunk_x, min_chunk_z),
            max_chunk: (max_chunk_x, max_chunk_z),
            current_chunk: (min_chunk_x, min_chunk_z),
            chunk_stride: chunks_x,
            position,
            distance_sq: distance * distance,
            empty,
        }
    }
}

impl <'a> Iterator for NearbyChunkIter<'a> {
    type Item = &'a Chunk;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.empty {
                return None;
            }

            let x = self.current_chunk.0;
            let z = self.current_chunk.1;

            debug_assert!(x >= self.min_chunk.0);
            debug_assert!(z >= self.min_chunk.1);
            debug_assert!(x <= self.max_chunk.0);
            debug_assert!(z <= self.max_chunk.1);

            let dx = self.position.x.clamp(x as f64 * 16.0, x as f64 * 16.0 + 16.0) - self.position.x;
            let dz = self.position.y.clamp(z as f64 * 16.0, z as f64 * 16.0 + 16.0) - self.position.y;

            if self.current_chunk.0 < self.max_chunk.0 {
                self.current_chunk.0 += 1;
            } else if self.current_chunk.1 < self.max_chunk.1 {
                self.current_chunk.0 = self.min_chunk.0;
                self.current_chunk.1 += 1;
            } else {
                self.empty = true;
            }

            if dx*dx + dz*dz <= self.distance_sq {
                return Some(&self.chunks[(x + z * self.chunk_stride) as usize]);
            }
        }
    }
}

pub struct NearbyChunkIterMut<'a> {
    chunks: &'a mut [Chunk],
    min_chunk: (i32, i32),
    max_chunk: (i32, i32),
    current_chunk: (i32, i32),
    chunk_stride: i32,

    position: DVec2,
    distance_sq: f64,
    empty: bool
}

impl <'a> NearbyChunkIterMut <'a> {
    pub fn new(chunks: &'a mut [Chunk], chunks_x: i32, chunks_z: i32, position: DVec2, distance: f64) -> Self {
        let min_chunk_x = (position.x - distance).floor() as i32 >> 4;
        let min_chunk_z = (position.y - distance).floor() as i32 >> 4;
        let max_chunk_x = (position.x + distance).floor() as i32 >> 4;
        let max_chunk_z = (position.y + distance).floor() as i32 >> 4;

        let min_chunk_x = min_chunk_x.max(0);
        let min_chunk_z = min_chunk_z.max(0);
        let max_chunk_x = max_chunk_x.min(chunks_x - 1);
        let max_chunk_z = max_chunk_z.min(chunks_z - 1);

        let empty = min_chunk_x > max_chunk_x || min_chunk_z > max_chunk_z;

        return Self {
            chunks,
            min_chunk: (min_chunk_x, min_chunk_z),
            max_chunk: (max_chunk_x, max_chunk_z),
            current_chunk: (min_chunk_x, min_chunk_z),
            chunk_stride: chunks_x,
            position,
            distance_sq: distance * distance,
            empty,
        }
    }
}

impl <'a> Iterator for NearbyChunkIterMut<'a> {
    type Item = &'a mut Chunk;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.empty {
                return None;
            }

            let x = self.current_chunk.0;
            let z = self.current_chunk.1;

            debug_assert!(x >= self.min_chunk.0);
            debug_assert!(z >= self.min_chunk.1);
            debug_assert!(x <= self.max_chunk.0);
            debug_assert!(z <= self.max_chunk.1);

            let dx = self.position.x.clamp(x as f64 * 16.0, x as f64 * 16.0 + 16.0) - self.position.x;
            let dz = self.position.y.clamp(z as f64 * 16.0, z as f64 * 16.0 + 16.0) - self.position.y;

            if self.current_chunk.0 < self.max_chunk.0 {
                self.current_chunk.0 += 1;
            } else if self.current_chunk.1 < self.max_chunk.1 {
                self.current_chunk.0 = self.min_chunk.0;
                self.current_chunk.1 += 1;
            } else {
                self.empty = true;
            }

            if dx*dx + dz*dz <= self.distance_sq {
                // SAFETY: Subsequent calls to next() cannot alias
                let chunk = unsafe {
                    &mut *(&mut self.chunks[(x + z * self.chunk_stride) as usize] as *mut Chunk)
                };
                return Some(chunk);
            }
        }
    }
}