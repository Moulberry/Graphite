use binary::slice_serialization::*;

use crate::identify_packets;
use crate::types::SignatureData;
use crate::IdentifiedPacket;
use num_enum::TryFromPrimitive;

identify_packets! {
    PacketId,
    Hello<'_> = 0x00
}

slice_serializable! {
    #[derive(Debug)]
    pub struct Hello<'a> {
        pub username: &'a str as SizedString<16>,
        pub signature_data: Option<SignatureData<'a>>,
        pub uuid: Option<u128> as Option<BigEndian>
    }
}
