use anyhow::bail;
use binary::slice_serialization::SliceSerializable;
use binary::varint;
use protocol::IdentifiedPacket;
use std::fmt::Debug;
use thiserror::Error;

use crate::network_handler::ByteSender;

pub fn send_packet<'a, I: Debug, T>(
    byte_sender: &mut ByteSender,
    packet: &'a T,
) -> anyhow::Result<()>
where
    T: SliceSerializable<'a, T> + IdentifiedPacket<I> + 'a,
{
    let expected_packet_size = T::get_write_size(T::maybe_deref(packet));
    if expected_packet_size > 2097148 {
        bail!("packet too large!");
    }

    // allocate necessary bytes
    let mut bytes = vec![0; 4 + expected_packet_size];

    // write packet data
    // note: invariant should be satisfied because we allocated at least `get_write_size` bytes
    let slice_after_writing = unsafe { T::write(&mut bytes[4..], T::maybe_deref(packet)) };
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
    println!(
        "buffer: {:?}",
        &bytes[varint_bytes_spare..4 + bytes_written]
    );

    byte_sender.send(Box::from(&bytes[varint_bytes_spare..4 + bytes_written]));

    Ok(())
}

pub enum PacketReadResult<'a> {
    Complete(&'a [u8]),
    Partial,
    Empty,
}

#[derive(Error, Debug)]
pub enum PacketReadBufferError {
    #[error("received packet exceeds maximum size of 2097148")]
    PacketTooBig,
}

const MAXIMUM_PACKET_SIZE: usize = 2097148;

pub fn try_read_packet<'a>(slice: &mut &'a [u8]) -> anyhow::Result<PacketReadResult<'a>> {
    let remaining = slice.len();

    if remaining == 0 {
        return Ok(PacketReadResult::Empty);
    } else if remaining >= 3 {
        // Packet must start with varint header specifying the amount of data
        let (packet_size, varint_header_bytes) = varint::decode::u21(slice)?;
        let packet_size = packet_size as usize;

        if packet_size > MAXIMUM_PACKET_SIZE {
            return Err(PacketReadBufferError::PacketTooBig.into());
        }

        let remaining = remaining - varint_header_bytes;
        if remaining >= packet_size {
            // Enough bytes to fully read, consume varint header & emit fully read packet
            let ret = PacketReadResult::Complete(
                &slice[varint_header_bytes..varint_header_bytes + packet_size],
            );

            *slice = &slice[varint_header_bytes + packet_size..];

            return Ok(ret);
        }
    } else if remaining == 2 && slice[0] == 1 {
        // Special case for packet of size 1
        // Enough bytes (2) to fully read
        let ret = PacketReadResult::Complete(&slice[1..2]);

        *slice = &slice[2..];

        return Ok(ret);
    }

    // Not enough bytes to fully read, emit [varint header + remaining data] as partial read
    Ok(PacketReadResult::Partial)
}
