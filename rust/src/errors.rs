use std::{
    any::type_name,
    error::Error,
    fmt::{self, Display, Formatter},
};

/// This is the catch-all error returned the library. It provides factory
/// functions that give the error a different print out. There is no way to
/// distinguish between the errors created by the factory functions; they are
/// all ServerErrors.
#[derive(Debug)]
pub struct ServerError(pub String);

impl ServerError {
    /// An empty ServerError
    pub fn new() -> Self {
        Self(String::from(""))
    }

    /// Wraps the provided error e in a ServerError
    pub fn from(e: impl Display) -> Self {
        Self(Self::chain_msg(type_name::<Self>(), e))
    }

    pub fn malformed_request(e: impl Display) -> Self {
        Self::from(Self::chain_msg("Malformed request", e))
    }

    pub fn unsupported_proto(e: impl Display) -> Self {
        Self::from(Self::chain_msg("Unsupported proto", e))
    }

    pub fn unsupported_method(e: impl Display) -> Self {
        Self::from(Self::chain_msg("Unsupported method", e))
    }

    fn chain_msg(prefix: &str, msg: impl Display) -> String {
        let msg = format!("{}", msg);
        match msg.as_str() {
            "" => format!("{}", prefix),
            m => format!("{}: {}", prefix, m),
        }
    }
}

impl Display for ServerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Error for ServerError {}

#[derive(Debug)]
pub struct HttpParseError(pub String);

impl HttpParseError {
    pub fn from(e: impl Error) -> Self {
        Self(format!("{}", e))
    }
}

impl Display for HttpParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", type_name::<Self>(), self.0)
    }
}

impl Error for HttpParseError {}
