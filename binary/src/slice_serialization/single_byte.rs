use super::*;
use std::mem::transmute;

pub enum Single {}

macro_rules! single_impl {
    ($typ:tt, $conv_from:tt, $conv_to:tt) => {
        impl SliceSerializable<'_, $typ> for Single {
            type CopyType = $typ;

            fn read(bytes: &mut &[u8]) -> anyhow::Result<$typ> {
                if bytes.is_empty() {
                    return Err(BinaryReadError::NotEnoughRemainingBytes.into());
                }

                #[allow(unused_unsafe)]
                let ret = unsafe { $conv_from(bytes[0]) };

                *bytes = &bytes[1..];
                Ok(ret)
            }

            fn get_write_size(_: $typ) -> usize {
                1
            }

            unsafe fn write<'b>(bytes: &'b mut [u8], data: $typ) -> &'b mut [u8] {
                debug_assert!(
                    !bytes.is_empty(),
                    "invariant: slice must contain at least 1 byte to perform read"
                );

                bytes[0] = $conv_to(data);
                &mut bytes[1..]
            }

            #[inline(always)]
            fn as_copy_type(t: &$typ) -> Self::CopyType {
                *t
            }
        }
    };
}

// u8
single_impl!(u8, noop, noop);

fn noop(byte: u8) -> u8 {
    byte
}

// i8
single_impl!(i8, transmute, transmute);

// bool
single_impl!(bool, byte_from_bool, byte_to_bool);

fn byte_from_bool(byte: u8) -> bool {
    byte != 0
}

fn byte_to_bool(bool: bool) -> u8 {
    if bool {
        1
    } else {
        0
    }
}
