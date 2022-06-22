pub trait SliceSerializable<'a, T = Self> {
    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<T>;
    fn get_write_size(data: &T) -> usize;
    unsafe fn write<'b>(bytes: &'b mut [u8], data: &T) -> &'b mut [u8];
}

pub enum VarInt {}
impl SliceSerializable<'_, i32> for VarInt {
    fn read(bytes: &mut &[u8]) -> anyhow::Result<i32> {
        super::slice_reader::read_varint(bytes)
    }

    fn get_write_size(_: &i32) -> usize {
        5 // todo: we could calculate the needed bits as a time->space tradeoff. probably not worth?
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &i32) -> &'b mut [u8] {
        super::slice_writer::write_varint_i32(bytes, *data)
    }
}

pub enum SizedStringWithMax<const M: usize> {}
impl <'a, const M: usize> SliceSerializable<'a, &'a str> for SizedStringWithMax<M> {
    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<&'a str> {
        super::slice_reader::read_string_with_max_size(bytes, M)
    }

    fn get_write_size(data: &&str) -> usize {
        5 + data.len()
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &&str) -> &'b mut [u8] {
        super::slice_writer::write_sized_string(bytes, data)
    }
}

pub enum BigEndian {}
impl SliceSerializable<'_, u16> for BigEndian {
    fn read(bytes: &mut &[u8]) -> anyhow::Result<u16> {
        super::slice_reader::read_u16(bytes)
    }

    fn get_write_size(_: &u16) -> usize {
        2
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], data: &u16) -> &'b mut [u8] {
        super::slice_writer::write_u16(bytes, *data)
    }
}

macro_rules! slice_serializable_composite {
    ( $struct_name:ident$(<$lt:lifetime>)?, $( $field_name:ident : $typ:ty as $wire:tt$(<$($generic:tt),*>)? ),* ) => {
        pub struct $struct_name$(<$lt>)? {
            $(pub $field_name: $typ,)*
        }   

        impl $(<$lt>)? SliceSerializable<'a> for $struct_name$(<$lt>)? {
            fn read(bytes: &mut &'a [u8]) -> anyhow::Result<$struct_name$(<$lt>)?> {
                Ok($struct_name {
                    $(
                        $field_name: $wire$(::<$($generic,)*>)?::read(bytes)?,
                    )*
                })
            }

            fn get_write_size(object: &$struct_name) -> usize {
                $(
                    $wire$(::<$($generic,)*>)?::get_write_size(&object.$field_name) +
                )*
                0
            }

            unsafe fn write<'b>(mut bytes: &'b mut [u8], object: &$struct_name) -> &'b mut [u8] {
                $(
                    bytes = $wire$(::<$($generic,)*>)?::write(bytes, &object.$field_name);
                )*
                bytes
            }
        }
    }
}

pub(crate) use slice_serializable_composite;