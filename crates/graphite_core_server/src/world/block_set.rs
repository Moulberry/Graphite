use std::ops::RangeInclusive;

use graphite_mc_constants::block::Block;

#[derive(Default)]
pub struct BlockSet {
    ranges: Vec<RangeInclusive<u16>>
}

impl BlockSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_block_state(&mut self, block_state: u16) {
        for range in &mut self.ranges {
            if range.contains(&block_state) {
                return;
            }

            let start = *range.start();
            let end = *range.end();

            if start > 0 && start-1 == block_state {
                *range = block_state ..= end;
                return;
            } else if end < u16::MAX && end+1 == block_state {
                *range = start ..= block_state;
                return;
            }
        }

        self.ranges.push(block_state ..= block_state);
    }

    pub fn add_block(&mut self, block: Block) {
        self.ranges.push(block.block_state_range());
    }

    pub fn optimize(&mut self) {
        self.ranges.sort_by_key(|k| *k.start());

        let mut combined: Vec<RangeInclusive<u16>> = Vec::new();

        for range in self.ranges.iter().cloned() {
            if let Some(last) = combined.last_mut() {
                if *range.start() <= *last.end() {
                    let new_end = u16::max(*last.end(), *range.end());
                    *last = *last.start() ..= new_end;
                    continue;
                }
            }

            combined.push(range);
        }

        combined.shrink_to_fit();
        self.ranges = combined;
    }

    pub fn contains(&self, block_state: u16) -> bool {
        for range in &self.ranges {
            if range.contains(&block_state) {
                return true;
            }
        }
        return false;
    }
}