use super::*;

impl<'a, T: 'a, S: SliceSerializable<'a, T>> SliceSerializable<'a, Option<T>> for Option<S> {
    type RefType = &'a Option<T>;
    
    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<Option<T>> {
        let is_present = Single::read(bytes)?;

        if is_present {
            Ok(Option::Some(S::read(bytes)?))
        } else {
            Ok(Option::None)
        }
    }

    fn get_write_size(option: &'a Option<T>) -> usize {
        if let Some(inner) = option {
            1 + S::get_write_size(S::maybe_deref(&inner))
        } else {
            1
        }
    }

    unsafe fn write<'b>(mut bytes: &'b mut [u8], option: &'a Option<T>) -> &'b mut [u8] {
        if let Some(inner) = option {
            bytes = <Single as SliceSerializable<bool>>::write(bytes, true);
            bytes = S::write(&mut bytes[1..], S::maybe_deref(&inner));
        } else {
            bytes[0] = 0;
            bytes = &mut bytes[1..];
        }
        bytes
    }

    #[inline(always)]
    fn maybe_deref(t: &'a Option<T>) -> Self::RefType {
        t
    }
}