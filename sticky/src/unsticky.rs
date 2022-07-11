pub unsafe trait Unsticky {
    type UnstuckType;

    fn update_pointer(&mut self, index: usize);

    fn unstick(self) -> Self::UnstuckType;
}