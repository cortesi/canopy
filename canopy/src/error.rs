use thiserror::Error;

#[derive(Error, Debug)]
pub enum CanopyError {
    #[error("unknown")]
    Unknown,
}
