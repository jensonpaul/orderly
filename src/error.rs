use std::fmt;

/// All errors that can originate inside the `orderly` library.
#[derive(Debug)]
pub enum Error {
    /// A WebSocket transport or protocol error.
    BadConnection(tungstenite::Error),
    /// A JSON deserialisation error.
    BadData(serde_json::Error),
    /// An OS-level I/O error.
    IoError(std::io::Error),
    /// An invalid socket address string.
    BadAddr(std::net::AddrParseError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadConnection(e) => write!(f, "WebSocket connection error: {e}"),
            Error::BadData(e)       => write!(f, "Data parse error: {e}"),
            Error::IoError(e)       => write!(f, "I/O error: {e}"),
            Error::BadAddr(e)       => write!(f, "Address parse error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<tungstenite::Error> for Error {
    fn from(e: tungstenite::Error) -> Self { Self::BadConnection(e) }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self { Self::BadData(e) }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self { Self::IoError(e) }
}

impl From<std::net::AddrParseError> for Error {
    fn from(e: std::net::AddrParseError) -> Self { Self::BadAddr(e) }
}
