use thiserror::Error;

#[derive(Error, Debug)]
#[error("initialize must be called on this prior to use")]
pub struct UninitializedError;
