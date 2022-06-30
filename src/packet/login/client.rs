use crate::binary::slice_serializable::*;

use crate::packet::identify_packets;
use crate::packet::IdentifiedPacket;
use derive_try_from_primitive::TryFromPrimitive;

identify_packets! {
    PacketId,
    LoginStart<'_> = 0x00
}

slice_serializable_composite! {
    LoginStartSignatureData<'a>,
    timestamp: i64 as BigEndian,
    public_key: &'a [u8] as SizedBlob,
    signature: &'a [u8] as SizedBlob
}

slice_serializable_composite! {
    LoginStart<'a>,
    username: &'a str as SizedString<16>,
    signature_data: Option<LoginStartSignatureData<'a>>
}
