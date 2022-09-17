pub trait Unsticky {
    type UnstuckType;

    fn update_pointer(&mut self);

    fn unstick(self) -> Self::UnstuckType;
}
