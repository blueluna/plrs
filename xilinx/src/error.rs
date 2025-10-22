use uio_rs;
/// Crate errors

/// Error
#[derive(Debug)]
pub enum Error {
    /// System error
    System,
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
    /// Cannot accept more data
    Full,
    /// Read from a empty storage
    UnderRun,
    /// Write to a full storage
    OverRun,
    /// The length register does not match the number of bytes written
    LengthMismatch,
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
