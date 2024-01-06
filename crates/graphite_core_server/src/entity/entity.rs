use std::ptr::NonNull;

use downcast_rs::Downcast;
use glam::DVec3;
use graphite_binary::slice_serialization::SliceSerializable;
use graphite_mc_protocol::IdentifiedPacket;
use graphite_network::PacketBuffer;

use crate::world::{WorldExtension, World, chunk::{Chunk, ChunkEntityRef}};

use super::entity_view_controller::EntityViewController;

pub trait GenericEntity: Downcast {
    fn tick(&mut self);
    fn write_spawn(&self, packet_buffer: &mut PacketBuffer);
    fn write_despawn(&self, despawn_list: &mut Vec<i32>, packet_buffer: &mut PacketBuffer);
}
downcast_rs::impl_downcast!(GenericEntity);

impl <E: EntityExtension + 'static> GenericEntity for Entity<E> {
    fn tick(&mut self) {
        <Entity<E>>::tick(self);
    }

    fn write_spawn(&self, packet_buffer: &mut PacketBuffer) {
        <Entity<E>>::write_spawn(self, packet_buffer);
    }

    fn write_despawn(&self, despawn_list: &mut Vec<i32>, packet_buffer: &mut PacketBuffer) {
        <Entity<E>>::write_despawn(self, despawn_list, packet_buffer);
    }
}

pub trait EntityExtension: Sized + 'static {
    type World: WorldExtension;
    type View: EntityViewController<Self>;

    fn tick(entity: &mut Entity<Self>);
    fn create_view_controller(&mut self) -> Self::View;
}

pub struct Entity<E: EntityExtension> {
    world: NonNull<World<E::World>>,

    pub(crate) synced_position: DVec3,
    last_position: DVec3,
    pub position: DVec3,

    pub(crate) last_chunk_x: i32,
    pub(crate) last_chunk_z: i32,
    pub(crate) chunk_ref: Option<ChunkEntityRef>,

    pub extension: E,
    pub view: E::View
}

impl <E: EntityExtension> Entity<E> {
    pub fn new(world: &mut World<E::World>, position: DVec3, mut extension: E) -> Self {
        let view = extension.create_view_controller();

        Self {
            world: world.into(),

            synced_position: position,
            last_position: position,
            position,

            last_chunk_x: (position.x.floor() as i32) >> 4,
            last_chunk_z: (position.z.floor() as i32) >> 4,
            chunk_ref: None,
            
            extension,
            view
        }
    }

    pub fn extension(&mut self) -> &mut E {
        &mut self.extension
    }

    pub fn add_viewable_packet<'a, I: std::fmt::Debug, T>(&mut self, packet: &'a T)
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<I> + 'a,
    {
        if let Some(chunk) = self.get_last_chunk_mut() {
            chunk.add_entity_viewable_packet(packet);
        }
    }

    pub fn get_last_chunk_mut(&mut self) -> Option<&mut Chunk> {
        let world = unsafe { self.world.as_mut() };
        world.get_chunk_mut(self.last_chunk_x, self.last_chunk_z)
    }

    fn tick(&mut self) {
        E::tick(self);

        self.update_position();
    }

    fn write_spawn(&self, packet_buffer: &mut PacketBuffer) {
        E::View::write_spawn_packets(self, packet_buffer);
    }

    fn write_despawn(&self, despawn_list: &mut Vec<i32>, packet_buffer: &mut PacketBuffer) {
        E::View::write_despawn_packets(self, despawn_list, packet_buffer);
    }

    fn update_position(&mut self) {
        if self.get_last_chunk_mut().is_some() {
            E::View::update_position(self);
        }

        self.last_position = self.position;
        self.last_chunk_x = (self.last_position.x.floor() as i32) >> 4;
        self.last_chunk_z = (self.last_position.z.floor() as i32) >> 4;
    }

    pub fn view(&mut self) -> &mut E::View {
        &mut self.view
    }
}