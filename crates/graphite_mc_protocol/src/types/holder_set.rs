use std::borrow::Cow;

use graphite_binary::slice_serialization::{Single, SizedString, SliceSerializable, VarInt};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum HolderSet<'a, T: Clone> {
    Single(T),
    Dynamic(Cow<'a, [T]>),
    Named(Cow<'a, str>)
}

impl <'r, 'd: 'r, T: 'd + Clone, S: Clone + SliceSerializable<'r, 'd, T>> SliceSerializable<'r, 'd, HolderSet<'d, T>> for HolderSet<'d, S> {
    type CopyType = &'r HolderSet<'d, T>;

    #[inline(always)]
    fn as_copy_type(t: &'r HolderSet<'d, T>) -> Self::CopyType {
        t
    }

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<HolderSet<'d, T>> {
        let value: i32 = VarInt::read(bytes)?;
        let value = value - 1;
        if value < 0 {
            let resource: &'d str = SizedString::<32767>::read(bytes)?;
            Ok(HolderSet::Named(Cow::Borrowed(resource)))
        } else if value == 1 {
            let t = S::read(bytes)?;
            Ok(HolderSet::Single(t))
        } else {
            let mut vec = Vec::with_capacity((value as usize).min(65536));
            for _ in 0 .. value {
                vec.push(S::read(bytes)?);
            }
            Ok(HolderSet::Dynamic(Cow::Owned(vec)))
        }
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        match data {
            HolderSet::Dynamic(vec) => {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, vec.len() as i32 + 1);
                for t in vec.iter() {
                    bytes = S::write(bytes, S::as_copy_type(t));
                }
                bytes
            },
            HolderSet::Single(t) => {
                bytes = <VarInt as SliceSerializable<i32>>::write(bytes, 2);
                bytes = S::write(bytes, S::as_copy_type(t));
                bytes
            }
            HolderSet::Named(resource) => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                <SizedString::<32767> as SliceSerializable<&str>>::write(bytes, resource)
            }
        }
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        let mut size = 0;
        match data {
            HolderSet::Dynamic(vec) => {
                size += <VarInt as SliceSerializable<i32>>::get_write_size(vec.len() as i32 + 1);
                for t in vec.iter() {
                    size += S::get_write_size(S::as_copy_type(t));
                }
            },
            HolderSet::Single(t) => {
                size += <VarInt as SliceSerializable<i32>>::get_write_size(2);
                size += S::get_write_size(S::as_copy_type(t));
            }
            HolderSet::Named(resource) => {
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <SizedString::<32767> as SliceSerializable<&str>>::get_write_size(resource);
            }
        }
        size
    }
}
