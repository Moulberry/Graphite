use std::sync::atomic::{AtomicI32, Ordering};

pub mod entity_view_controller;
mod entity;

pub use entity::*;

static ENTITY_ID: AtomicI32 = AtomicI32::new(1);

pub fn next_entity_id() -> i32 {
    ENTITY_ID.fetch_add(1, Ordering::SeqCst)
}
