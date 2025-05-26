use super::*;

#[derive(Clone)]
pub struct AttemptFrom<S, F> {
    _phantom1: S,
    _phantom2: F,
}

impl<'r, 'd: 'r, F, T: TryFrom<F> + Into<F> + Copy, S: SliceSerializable<'r, 'd, F, CopyType = F>>
    SliceSerializable<'r, 'd, T> for AttemptFrom<S, F>
{
    type CopyType = T;

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<T> {
        let intermediate = S::read(bytes)?;
        T::try_from(intermediate).map_err(|_| anyhow::anyhow!("try_from failed"))
    }

    fn get_write_size(t: T) -> usize {
        S::get_write_size(T::into(t))
    }

    unsafe fn write(bytes: &mut [u8], t: T) -> &mut [u8] {
        S::write(bytes, T::into(t))
    }

    #[inline(always)]
    fn as_copy_type(t: &T) -> Self::CopyType {
        *t
    }
}