use std::fmt::Debug;

pub mod handshake;
pub mod login;
pub mod configuration;
pub mod play;
pub mod status;
pub mod types;

pub trait IdentifiedPacket<I: Debug>: Debug {
    const ID: I;

    fn get_packet_id(&self) -> I;
    fn get_packet_id_as_u8(&self) -> u8;
}

macro_rules! identify_packets {
    { $enum_name:ident, $( $packet:ident $(<$life:lifetime>)? = $val:tt ),* } => {
        #[derive(Debug, TryFromPrimitive, Eq, PartialEq)]
        #[repr(u8)]
        pub enum $enum_name {
            $( $packet = $val,)*
        }

        $(impl IdentifiedPacket<$enum_name> for $packet $(<$life>)? {
            const ID: $enum_name = $enum_name::$packet;

            fn get_packet_id(&self) -> $enum_name {
                Self::ID
            }
            fn get_packet_id_as_u8(&self) -> u8 {
                Self::ID as u8
            }
        })*

        pub fn debug_print_packet(mut bytes: &[u8]) -> String {
            let packet_id_byte: u8 = graphite_binary::slice_serialization::Single::read(&mut bytes)
                .expect("packet must start with varint for the id");

            if let Ok(packet_id) = $enum_name::try_from(packet_id_byte) {
                match packet_id {
                    $(
                        $enum_name::$packet => {
                            let packet = $packet::read_fully(&mut bytes)
                                .expect(&format!("unable to read packet by id: 0x{:x}", packet_id_byte));
                            return format!("{:?}", packet);
                        }
                    )*
                }
            } else {
                panic!("unknown packet_id 0x{:x}", packet_id_byte);
            }
        }

        pub fn debug_handle_packet<'a, T, F>(mut bytes: &'a [u8], func: F)
        where
            F: FnOnce(&mut T),
            T: IdentifiedPacket<PacketId> + 'a
        {
            let packet_id_byte: u8 = graphite_binary::slice_serialization::Single::read(&mut bytes)
                .expect("packet must start with varint for the id");

            if let Ok(packet_id) = $enum_name::try_from(packet_id_byte) {
                match packet_id {
                    $(
                        $enum_name::$packet => {
                            let mut packet = $packet::read_fully(&mut bytes)
                                .expect(&format!("unable to read packet by id: 0x{:x}", packet_id_byte));
                            if packet.get_packet_id() != T::ID {
                                panic!("expected packet by id: 0x{:x}, got 0x{:x} instead", T::ID as u8, packet_id_byte);
                            }
                            (func)(unsafe { &mut *(&mut packet as *mut _ as *mut T) })
                        }
                    )*
                }
            } else {
                panic!("unknown packet_id 0x{:x}", packet_id_byte);
            }
        }

        pub trait PacketHandler {
            const DEBUG: bool = false;

            paste::paste! {
                $(
                    fn [<handle_ $packet:snake>](&mut self, _: $packet) -> anyhow::Result<()> {
                        Ok(())
                    }
                )*
            }

            fn parse_and_handle(&mut self, mut bytes: &[u8]) -> anyhow::Result<()> {
                let packet_id_byte: u8 = graphite_binary::slice_serialization::Single::read(&mut bytes)?;

                if let Ok(packet_id) = $enum_name::try_from(packet_id_byte) {
                    match packet_id {
                        $(
                            $enum_name::$packet => {
                                let packet_result = $packet::read_fully(&mut bytes);

                                match packet_result {
                                    Ok(packet) => {
                                        if Self::DEBUG {
                                            println!("<= {:?}", packet);
                                        }
                                        paste::paste! {
                                            Ok(self.[<handle_ $packet:snake>](packet)?)
                                        }
                                    }
                                    Err(err) => {
                                        println!("Error while parsing packet: 0x{:x}", packet_id_byte);
                                        Err(err)
                                    }
                                }
                            }
                        )*
                    }
                } else {
                    if Self::DEBUG {
                        println!("<= Unknown packet 0x{:x}", packet_id_byte)
                    }
                    Ok(())
                    // anyhow::bail!("unknown packet_id 0x{:x}", packet_id_byte)
                }
            }
        }
    }
}

pub(crate) use identify_packets;
