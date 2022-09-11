/// # Safety
/// `update_pointer` must update all pointers to `self` that exist
pub unsafe trait Unsticky {
    type UnstuckType;

    fn update_pointer(&mut self, index: usize);

    fn unstick(self) -> Self::UnstuckType;
}
