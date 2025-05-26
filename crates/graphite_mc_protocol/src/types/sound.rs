use graphite_binary::slice_serialization::*;
use graphite_mc_constants::builtin::SoundEvent;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SoundType<'a> {
    Event(SoundEvent),
    Direct(&'a str)
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd> for SoundType<'d> {
    type CopyType = Self;

    #[inline(always)]
    fn as_copy_type(t: &'r Self) -> Self::CopyType {
        *t
    }

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Self> {
        let value: i32 = VarInt::read(bytes)?;
        if value <= 0 {
            let resource: &'d str = SizedString::<32767>::read(bytes)?;
            let _: bool = Single::read(bytes)?;
            Ok(SoundType::Direct(resource))
        } else {
            let event: SoundEvent = (value as u16 - 1).try_into()?;
            Ok(SoundType::Event(event))
        }
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        match data {
            SoundType::Event(event) => {
                <VarInt as SliceSerializable<i32>>::write(bytes, event as i32 + 1)
            },
            SoundType::Direct(resource) => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <SizedString::<32767> as SliceSerializable<&str>>::write(bytes, resource);
                <Single as SliceSerializable<bool>>::write(bytes, false)
            }
        }
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        match data {
            SoundType::Event(event) => {
                <VarInt as SliceSerializable<i32>>::get_write_size(event as i32 + 1)
            },
            SoundType::Direct(resource) => {
                let mut size = 0;
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <SizedString::<32767> as SliceSerializable<&str>>::get_write_size(resource);
                size += <Single as SliceSerializable<bool>>::get_write_size(false);
                size
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SoundTypeOwned {
    Event(SoundEvent),
    Direct(String)
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd> for SoundTypeOwned {
    type CopyType = &'r Self;

    #[inline(always)]
    fn as_copy_type(t: &'r Self) -> Self::CopyType {
        t
    }

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Self> {
        let value: i32 = VarInt::read(bytes)?;
        if value <= 0 {
            let resource: &'d str = SizedString::<32767>::read(bytes)?;
            let _: bool = Single::read(bytes)?;
            Ok(Self::Direct(resource.to_string()))
        } else {
            let event: SoundEvent = (value as u16 - 1).try_into()?;
            Ok(Self::Event(event))
        }
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        match data {
            Self::Event(event) => {
                <VarInt as SliceSerializable<i32>>::write(bytes, *event as i32 + 1)
            },
            Self::Direct(resource) => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <SizedString::<32767> as SliceSerializable<&str>>::write(bytes, resource.as_str());
                <Single as SliceSerializable<bool>>::write(bytes, false)
            }
        }
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        match data {
            Self::Event(event) => {
                <VarInt as SliceSerializable<i32>>::get_write_size(*event as i32 + 1)
            },
            Self::Direct(resource) => {
                let mut size = 0;
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <SizedString::<32767> as SliceSerializable<&str>>::get_write_size(resource);
                size += <Single as SliceSerializable<bool>>::get_write_size(false);
                size
            }
        }
    }
}

