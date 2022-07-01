use anyhow::bail;
use binary::slice_serializable::SliceSerializable;
use binary::varint;
use protocol::IdentifiedPacket;
use std::fmt::Debug;
use std::io::Write;
use std::net::TcpStream;

pub fn send_packet<'a, I: Debug, T>(stream: &mut TcpStream, packet: &'a T) -> anyhow::Result<()>
where
    T: SliceSerializable<'a, T> + IdentifiedPacket<I> + 'a,
{
    let expected_packet_size = T::get_write_size(packet);
    if expected_packet_size > 2097148 {
        bail!("packet too large!");
    }

    // allocate necessary bytes
    let mut bytes = vec![0; 4 + expected_packet_size];

    // write packet data
    // note: invariant should be satisfied because we allocated at least `get_write_size` bytes
    let slice_after_writing = unsafe { T::write(&mut bytes[4..], packet) };
    let bytes_written = expected_packet_size - slice_after_writing.len();

    // encode packet size varint for [packet id size (1) + content size]
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
    println!(
        "sending: {:?} (0x{:x})",
        packet.get_packet_id(),
        packet.get_packet_id_as_u8()
    );
    println!("buffer: {:?}", &bytes[4..4 + bytes_written]);

    stream.write_all(&bytes[varint_bytes_spare..4 + bytes_written])?;
    stream.flush()?; // todo: is this needed?

    Ok(())
}
