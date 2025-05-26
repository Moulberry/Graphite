use super::{chunk_section::ChunkSection, BlockGetter, LightSetter};

#[derive(Clone)]
pub struct ChunkList {
    pub size_x: usize,
    pub size_y: usize,
    pub size_z: usize,
    pub chunks: Vec<Vec<ChunkSection>>,
}

impl BlockGetter for ChunkList {
    fn get_block(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        let Some(sections) = self.get_chunk(x >> 4, z >> 4) else {
            return None;
        };

        if y < 0 {
            return None;
        }

        let chunk_y = (y >> 4) as usize;
        if chunk_y >= sections.len() {
            return None; // out of bounds
        }

        let section = &sections[chunk_y];
        Some(section.get_block((x & 0xF) as _, (y & 0xF) as _, (z & 0xF) as _))
    }

    fn get_chunk_section(&self, chunk_x: i32, chunk_y: i32, chunk_z: i32) -> Option<&ChunkSection> {
        let Some(sections) = self.get_chunk(chunk_x, chunk_z) else {
            return None;
        };

        if chunk_y < 0 {
            return None;
        }

        let chunk_y = chunk_y as usize;
        if chunk_y >= sections.len() {
            return None; // out of bounds
        }

        let section = &sections[chunk_y];
        Some(section)
    }

    fn chunks_x(&self) -> usize {
        self.size_x
    }

    fn chunks_y(&self) -> usize {
        self.size_y
    }

    fn chunks_z(&self) -> usize {
        self.size_z
    }
}

impl LightSetter for ChunkList {
    fn set_block_light_array(&mut self, section_x: usize, section_y: usize, section_z: usize, light: Box<[u8]>) {
        if let Some(sections) = self.get_chunk_mut(section_x as i32, section_z as i32) {
            if section_y >= sections.len() {
                return; // out of bounds
            }

            sections[section_y].block_light = Some(light);
        }
    }

    fn set_sky_light_array(&mut self, section_x: usize, section_y: usize, section_z: usize, light: Box<[u8]>) {
        if let Some(sections) = self.get_chunk_mut(section_x as i32, section_z as i32) {
            if section_y >= sections.len() {
                return; // out of bounds
            }

            sections[section_y].sky_light = Some(light);
        }
    }
}

impl ChunkList {
    pub fn get_chunk(&self, x: i32, z: i32) -> Option<&Vec<ChunkSection>> {
        if x < 0 || z < 0 || x >= self.size_x as i32 || z >= self.size_z as i32 {
            None
        } else {
            Some(&self.chunks[(x + z * self.size_x as i32) as usize])
        }
    }

    pub fn get_chunk_mut(&mut self, x: i32, z: i32) -> Option<&mut Vec<ChunkSection>> {
        if x < 0 || z < 0 || x >= self.size_x as i32 || z >= self.size_z as i32 {
            None
        } else {
            Some(&mut self.chunks[(x + z * self.size_x as i32) as usize])
        }
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: u16) {
        let Some(sections) = self.get_chunk_mut(x >> 4, z >> 4) else {
            return;
        };

        if y < 0 {
            return;
        }

        let chunk_y = (y >> 4) as usize;
        if chunk_y >= sections.len() {
            return; // out of bounds
        }

        let section = &mut sections[chunk_y];
        section.set_block((x & 0xF) as _, (y & 0xF) as _, (z & 0xF) as _, block);
    }

    pub fn expand(&mut self, increase_x: i32, increase_y: i32, increase_z: i32) {
        if increase_y != 0 {
            let new_size = self.size_y + increase_y.abs() as usize;

            for chunk in &mut self.chunks {
                let additional = increase_y.abs() as usize;
                chunk.reserve_exact(additional);

                if increase_y < 0 {
                    chunk.splice(0..0, std::iter::repeat(ChunkSection::new_empty()).take(additional));
                } else {
                    chunk.resize_with(chunk.len() + additional, || ChunkSection::new_empty());
                }
            }

            self.size_y = new_size;
        }

        if increase_x == 0 && increase_z == 0 {
            return;
        }

        let empty = vec![ChunkSection::new_empty(); self.size_y];

        if increase_x != 0 {
            // X = New value
            // _ =  Uninitialized
            // - =  Old Value
            
            // 1. (start)   ----,----,----
            // 2. (reserve) ----,----,----___
            // 3. (i = 0)   ----,----,__----X
            // 4. (i = 1)   ----,_----X,----X
            // 5. (i = 2)   ----X,----X,----X

            // note: a negative value of increase_x does not reduce the size,
            // instead it expands the front
            // eg. X----,X----,X----

            assert_eq!(self.chunks.len(), self.size_x * self.size_z);

            let abs_increase_x = increase_x.abs() as usize;
            let new_size_x = self.size_x + abs_increase_x;
            let new_chunk_count = abs_increase_x * self.size_z;
            self.chunks.reserve_exact(new_chunk_count);

            for i in (0..self.size_z).rev() {
                unsafe {
                    // Copy existing chunks from (src_index..src_index+size_x) to (dst_index..dst_index+size_x)
                    let src_index = i * self.size_x;

                    let dst_index = if increase_x > 0 {
                        i * new_size_x
                    } else {
                        i * new_size_x + abs_increase_x
                    };

                    let src = self.chunks.as_mut_ptr().add(src_index);
                    let dst = self.chunks.as_mut_ptr().add(dst_index);
                    std::ptr::copy(src, dst, self.size_x);

                    // Put new chunks (end..end+abs_increase_x)
                    let new_index = if increase_x > 0 {
                        i * new_size_x + self.size_x
                    } else {
                        i * new_size_x
                    };

                    for i in 0..abs_increase_x {
                        let dst = self.chunks.as_mut_ptr().add(new_index + i);
                        std::ptr::write(dst, empty.clone());
                    }
                }
            }

            unsafe {
                self.chunks.set_len(self.chunks.len() + new_chunk_count);
            }
            self.size_x = new_size_x;
        }

        if increase_z != 0 {
            let abs_increase_z = increase_z.abs() as usize;
            let new_chunk_count = abs_increase_z * self.size_x;
            
            self.chunks.reserve_exact(new_chunk_count);

            if increase_z > 0 {
                self.chunks.resize(self.chunks.len() + new_chunk_count, empty);
            } else {
                self.chunks.splice(0..0, std::iter::repeat(empty).take(new_chunk_count));
            }

            self.size_z += abs_increase_z;
        }
    }

    pub fn rotate_clockwise(&self) -> Self {
        self.rotate_clockwise_with_function(|id| {
            let attributes = graphite_mc_constants::block::BlockAttributes::from_block_state(id);
            (id as i16 + attributes.clockwise_rotation_state_id_offset) as u16
        })
    }

    pub fn rotate_clockwise_with_function<F>(&self, rotation: F) -> Self
        where F: Fn(u16) -> u16
    {
        let mut chunks = Vec::with_capacity(self.chunks.len());

        for x in 0..self.size_x as i32 {
            for z in (0..self.size_z as i32).rev() {
                let chunk = &self.chunks[(x + z * self.size_x as i32) as usize];

                let rotated = chunk.iter().map(|section| section.rotate_clockwise(&rotation)).collect();

                chunks.push(rotated);
            }   
        }

        Self {
            size_x: self.size_z,
            size_y: self.size_y,
            size_z: self.size_x,
            chunks,
        }
    }
}