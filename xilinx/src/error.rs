use uio_rs;
/// Crate errors

/// Error
#[derive(Debug)]
pub enum Error {
    /// No device found
    NoDevice,
    /// Failed to lock device
    DeviceLock,
    /// Invalid address
    Address,
    /// Parse error
    Parse,
    /// Value out of bounds
    OutOfBound,
    /// No data available
    Empty,
    /// Underlying IO error
    Io(std::io::Error),
    /// Underlying UIO error
    Uio(uio_rs::Error),
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Io(error)
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(_: std::num::ParseIntError) -> Self {
        Error::Parse
    }
}

impl From<uio_rs::Error> for Error {
    fn from(error: uio_rs::Error) -> Self {
        Error::Uio(error)
    }
}
