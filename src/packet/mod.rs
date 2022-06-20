pub mod handshake;
pub mod login;

pub trait Packet<'a, T> {
    fn read(bytes: &'a [u8]) -> anyhow::Result<T>;
    fn write(&self) -> Vec<u8>;
}