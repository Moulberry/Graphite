use super::*;

pub struct AttemptFrom<S, F> {
    _phantom1: S,
    _phantom2: F,
}

impl<'a, F, T: TryFrom<F> + Into<F> + Copy, S: SliceSerializable<'a, F, RefType = F>>
    SliceSerializable<'a, T> for AttemptFrom<S, F>
{
    type RefType = T;

    fn read(bytes: &mut &'a [u8]) -> anyhow::Result<T> {
        let intermediate = S::read(bytes)?;
        T::try_from(intermediate).map_err(|_| anyhow::anyhow!("try_from failed"))
    }

    fn get_write_size(t: T) -> usize {
        S::get_write_size(T::into(t))
    }

    unsafe fn write<'b>(bytes: &'b mut [u8], t: T) -> &'b mut [u8] {
        S::write(bytes, T::into(t))
    }

    #[inline(always)]
    fn maybe_deref(t: &T) -> Self::RefType {
        *t
    }
}
