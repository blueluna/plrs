use uio_rs;
/// Crate errors

/// Error
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Error {
    /// No memory map found
    NoMemoryMap,
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
    Io(std::io::ErrorKind),
    /// Underlying UIO error
    Uio(uio_rs::Error),
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Io(error.kind())
    }
}

impl From<uio_rs::Error> for Error {
    fn from(error: uio_rs::Error) -> Self {
        Error::Uio(error)
    }
}
