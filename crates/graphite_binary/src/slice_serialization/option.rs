use super::*;

impl<'a, T: 'a, S: SliceSerializable<'a, T>> SliceSerializable<'a, Option<T>> for Option<S> {
    type CopyType = &'a Option<T>;

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<Option<T>> {
        let is_present = Single::read(bytes)?;

        if is_present {
            Ok(Option::Some(S::read(bytes)?))
        } else {
            Ok(Option::None)
        }
    }

    #[allow(clippy::needless_borrow)] // maybe_deref is needed for some types
    fn get_write_size(option: &'a Option<T>) -> usize {
        if let Some(inner) = option {
            1 + S::get_write_size(S::as_copy_type(&inner))
        } else {
            1
        }
    }

    #[allow(clippy::needless_borrow)] // maybe_deref is needed for some types
    unsafe fn write<'b>(mut bytes: &'b mut [u8], option: &'a Option<T>) -> &'b mut [u8] {
        if let Some(inner) = option {
            bytes[0] = 1;
            bytes = S::write(&mut bytes[1..], S::as_copy_type(&inner));
        } else {
            bytes[0] = 0;
            bytes = &mut bytes[1..];
        }
        bytes
    }

    #[inline(always)]
    fn as_copy_type(t: &'a Option<T>) -> Self::CopyType {
        t
    }
}
