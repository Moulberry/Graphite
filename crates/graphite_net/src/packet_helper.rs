use crate::network_buffer::WriteBuffer;
use anyhow::bail;
use graphite_binary::slice_serialization::{SliceSerializable, VarInt};
use graphite_binary::varint;
use graphite_mc_constants::entity::Metadata;
use graphite_mc_protocol::IdentifiedPacket;
use std::fmt::Debug;
use thiserror::Error;

pub fn write_slice_serializable<'a, T>(write_buffer: &mut WriteBuffer, serializable: &'a T)
where
    T: SliceSerializable<'a, T>,
{
    let ref_type = T::as_copy_type(serializable);

    let expected_size = T::get_write_size(ref_type.clone());

    // allocate necessary bytes
    let bytes = write_buffer.get_unwritten(expected_size);

    // write the serializable
    let slice_after_writing = unsafe { T::write(bytes, ref_type) };
    let bytes_written = expected_size - slice_after_writing.len();

    // advance the write buffer
    unsafe {
        write_buffer.advance(bytes_written);
    }
}

pub fn write_metadata_packet<T>(
    write_buffer: &mut WriteBuffer,
    packet_id: u8,
    entity_id: i32,
    metadata: &mut T,
) -> anyhow::Result<()>
where
    T: Metadata,
{
    let expected_packet_size = 5 + metadata.get_write_size();
    if expected_packet_size > 2097148 {
        bail!("packet too large!");
    }

    // allocate necessary bytes
    let bytes = write_buffer.get_unwritten(4 + expected_packet_size);

    // write packet data
    // safety: invariant should be satisfied because we allocated at least `get_write_size` bytes
    let contents = &mut bytes[4..];
    let contents = unsafe { <VarInt as SliceSerializable<i32>>::write(contents, entity_id) };
    let contents = unsafe { metadata.write_changes(contents) };
    let bytes_written = expected_packet_size - contents.len();

    // encode packet size varint for [packet id size (1) + content size]
    let (varint_raw, varint_bytes) = varint::encode::i32_raw(1 + bytes_written as i32);
    if varint_bytes > 3 {
        bail!("packet too large!");
    }

    // write packet size varint
    bytes[0..varint_bytes].copy_from_slice(&varint_raw[..varint_bytes]);

    if varint_bytes == 1 {
        bytes[0] |= 0b10000000;
        bytes[1] = 0b10000000;
        bytes[2] = 0b00000000;
    } else if varint_bytes == 2 {
        bytes[1] |= 0b10000000;
        bytes[2] = 0b00000000;
    }

    // write packet id
    bytes[3] = packet_id;

    unsafe {
        // advance write buffer
        write_buffer.advance(4 + bytes_written);
    }

    Ok(())
}

pub fn write_custom_packet<'a, T>(
    write_buffer: &mut WriteBuffer,
    packet_id: u8,
    serializable: &'a T,
) -> anyhow::Result<()>
where
    T: SliceSerializable<'a, T>,
{
    let expected_packet_size = T::get_write_size(T::as_copy_type(serializable));
    if expected_packet_size > 2097148 {
        bail!("packet too large!");
    }

    // allocate necessary bytes
    let bytes = write_buffer.get_unwritten(4 + expected_packet_size);

    // write packet data
    // note: invariant should be satisfied because we allocated at least `get_write_size` bytes
    let slice_after_writing = unsafe { T::write(&mut bytes[4..], T::as_copy_type(serializable)) };
    let bytes_written = expected_packet_size - slice_after_writing.len();

    // encode packet size varint for [packet id size (1) + content size]
    let (varint_raw, varint_bytes) = varint::encode::i32_raw(1 + bytes_written as i32);
    if varint_bytes > 3 {
        bail!("packet too large!");
    }

    // write packet size varint
    bytes[0..varint_bytes].copy_from_slice(&varint_raw[..varint_bytes]);

    if varint_bytes == 1 {
        bytes[0] |= 0b10000000;
        bytes[1] = 0b10000000;
        bytes[2] = 0b00000000;
    } else if varint_bytes == 2 {
        bytes[1] |= 0b10000000;
        bytes[2] = 0b00000000;
    }

    // write packet id
    bytes[3] = packet_id;

    unsafe {
        // advance write buffer
        write_buffer.advance(4 + bytes_written);
    }

    Ok(())
}

pub fn write_packet<'a, I: Debug, T>(
    write_buffer: &mut WriteBuffer,
    packet: &'a T,
) -> anyhow::Result<()>
where
    T: SliceSerializable<'a, T> + IdentifiedPacket<I> + 'a,
{
    write_custom_packet(write_buffer, packet.get_packet_id_as_u8(), packet)
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
