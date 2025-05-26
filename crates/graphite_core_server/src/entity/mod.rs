mod entity;
use std::sync::atomic::{AtomicI32, Ordering};

pub use entity::*;

pub mod entity_view;
pub mod remote_entity;

static ENTITY_ID: AtomicI32 = AtomicI32::new(1);

pub fn next_entity_id() -> i32 {
    ENTITY_ID.fetch_add(1, Ordering::SeqCst)
}

pub fn next_entity_ids(count: usize) -> Vec<i32> {
    let mut ids = Vec::with_capacity(count);
    let count = count as i32;
    let from = ENTITY_ID.fetch_add(count, Ordering::SeqCst);
    let to = from + count;
    for id in from..to {
        ids.push(id);
    }
    ids
}