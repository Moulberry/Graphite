use glam::{DVec3, Quat};
use graphite_binary::slice_serialization::*;

// Byte Rotation

pub enum ByteRotation {}

impl ByteRotation {
    pub fn to_f32(byte: u8) -> f32 {
        byte as f32 * 360.0 / 256.0
    }

    pub fn from_f32(float: f32) -> u8 {
        (float * 256.0 / 360.0) as i64 as u8
    }
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd, f32> for ByteRotation {
    type CopyType = f32;

    fn as_copy_type(t: &f32) -> Self::CopyType {
        *t
    }

    fn read(bytes: &mut &[u8]) -> anyhow::Result<f32> {
        let byte: u8 = Single::read(bytes)?;
        Ok(Self::to_f32(byte))
    }

    unsafe fn write(bytes: &mut [u8], data: f32) -> &mut [u8] {
        let byte = Self::from_f32(data);
        <Single as SliceSerializable<u8>>::write(bytes, byte)
    }

    fn get_write_size(_: f32) -> usize {
        1
    }
}

// Quantized Short

pub enum QuantizedShort {}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd, f32> for QuantizedShort {
    type CopyType = f32;

    fn as_copy_type(t: &f32) -> Self::CopyType {
        *t
    }

    fn read(bytes: &mut &[u8]) -> anyhow::Result<f32> {
        let short: i16 = BigEndian::read(bytes)?;
        Ok(short as f32 / 8000.0)
    }

    unsafe fn write(bytes: &mut [u8], data: f32) -> &mut [u8] {
        let short = (data * 8000.0).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        <BigEndian as SliceSerializable<i16>>::write(bytes, short)
    }

    fn get_write_size(_: f32) -> usize {
        2
    }
}

pub enum QuantizedInt<const FACTOR: i32> {}

impl <'r, 'd: 'r, const FACTOR: i32> SliceSerializable<'r, 'd, f32> for QuantizedInt<FACTOR> {
    type CopyType = f32;

    fn as_copy_type(t: &f32) -> Self::CopyType {
        *t
    }

    fn read(bytes: &mut &[u8]) -> anyhow::Result<f32> {
        let int: i32 = BigEndian::read(bytes)?;
        Ok(int as f32 / FACTOR as f32)
    }

    unsafe fn write(bytes: &mut [u8], data: f32) -> &mut [u8] {
        let int = (data * FACTOR as f32).clamp(i32::MIN as f32, i32::MAX as f32) as i32;
        <BigEndian as SliceSerializable<i32>>::write(bytes, int)
    }

    fn get_write_size(_: f32) -> usize {
        4
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct Position {
        pub x: f64 as BigEndian,
        pub y: f64 as BigEndian,
        pub z: f64 as BigEndian,
    }
}

pub enum OptionalVarInt {}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd, Option<i32>> for OptionalVarInt {
    type CopyType = Option<i32>;

    fn as_copy_type(t: &Option<i32>) -> Self::CopyType {
        *t
    }

    fn read(bytes: &mut &[u8]) -> anyhow::Result<Option<i32>> {
        let int: i32 = VarInt::read(bytes)?;
        if int == 0 {
            Ok(None)
        } else {
            Ok(Some(int - 1))
        }
    }

    unsafe fn write(bytes: &mut [u8], data: Option<i32>) -> &mut [u8] {
        <VarInt as SliceSerializable<i32>>::write(bytes, data.unwrap_or(-1) + 1)
    }

    fn get_write_size(data: Option<i32>) -> usize {
        <VarInt as SliceSerializable<i32>>::get_write_size(data.unwrap_or(-1) + 1)
    }
}

pub struct DVec3Serializer;
impl <'r, 'd: 'r> SliceSerializable<'r, 'd, DVec3> for DVec3Serializer {
    type CopyType = DVec3;

    fn as_copy_type(t: &DVec3) -> Self::CopyType {
        *t
    }

    fn read(bytes: &mut &[u8]) -> anyhow::Result<DVec3> {
        let x = <BigEndian as SliceSerializable<f64>>::read(bytes)?;
        let y = <BigEndian as SliceSerializable<f64>>::read(bytes)?;
        let z = <BigEndian as SliceSerializable<f64>>::read(bytes)?;
        Ok(DVec3::new(x, y, z))
    }

    unsafe fn write(mut bytes: &mut [u8], data: DVec3) -> &mut [u8] {
        bytes = <BigEndian as SliceSerializable<f64>>::write(bytes, data.x);
        bytes = <BigEndian as SliceSerializable<f64>>::write(bytes, data.y);
        bytes = <BigEndian as SliceSerializable<f64>>::write(bytes, data.z);
        bytes
    }

    fn get_write_size(_: DVec3) -> usize {
        24
    }
}

pub struct QuatSerializer;
impl <'r, 'd: 'r> SliceSerializable<'r, 'd, Quat> for QuatSerializer {
    type CopyType = Quat;

    fn as_copy_type(t: &Quat) -> Self::CopyType {
        *t
    }

    fn read(bytes: &mut &[u8]) -> anyhow::Result<Quat> {
        let x = <BigEndian as SliceSerializable<f32>>::read(bytes)?;
        let y = <BigEndian as SliceSerializable<f32>>::read(bytes)?;
        let z = <BigEndian as SliceSerializable<f32>>::read(bytes)?;
        let w = <BigEndian as SliceSerializable<f32>>::read(bytes)?;
        Ok(Quat::from_xyzw(x, y, z, w))
    }

    unsafe fn write(mut bytes: &mut [u8], data: Quat) -> &mut [u8] {
        bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, data.x);
        bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, data.y);
        bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, data.z);
        bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, data.w);
        bytes
    }

    fn get_write_size(_: Quat) -> usize {
        16
    }
}