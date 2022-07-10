use binary::slice_serialization::*;

use crate::identify_packets;
use crate::IdentifiedPacket;
use derive_try_from_primitive::TryFromPrimitive;

identify_packets! {
    PacketId,
    Hello<'_> = 0x00
}

slice_serializable_composite! {
    LoginStartSignatureData<'a>,
    timestamp: i64 as BigEndian,
    public_key: &'a [u8] as SizedBlob,
    signature: &'a [u8] as SizedBlob
}

slice_serializable_composite! {
    Hello<'a>,
    username: &'a str as SizedString<16>,
    signature_data: Option<LoginStartSignatureData<'a>>,
    uuid: Option<u128> as Option<BigEndian>
}
