use anyhow::bail;
use std::net::TcpStream;
use std::io::Write;
use crate::binary::varint;
use crate::packet::Packet;

pub fn send_packet<'a, I, T: Packet<'a, I>>(stream: &mut TcpStream, packet: T) -> anyhow::Result<()> {
    let expected_packet_size = packet.get_write_size();
    if expected_packet_size > 2097148 {
        bail!("packet too large!");
    }

    // allocate necessary bytes
    let mut bytes = vec![0; 4 + expected_packet_size];

    // write packet data
    // note: invariant should be satisfied because we allocated at least `get_write_size` bytes
    let slice_after_writing = unsafe { packet.write(&mut bytes[4..]) };
    let bytes_written = expected_packet_size - slice_after_writing.len();

    // get encoded varint for remaining size
    let (varint_raw, written) = varint::encode::i32_raw(1 + bytes_written as i32);
    if written > 3 {
        bail!("packet too large!");
    }

    // write packet size varint
    let varint_bytes_spare = 3 - written;
    bytes[varint_bytes_spare..3].copy_from_slice(&varint_raw[..written]);

    // write packet id
    bytes[3] = packet.get_packet_id_as_u8();

    // write buffer to stream
    stream.write_all(&bytes[varint_bytes_spare..4+bytes_written])?;
    stream.flush()?; // todo: is this needed?

    Ok(())
}