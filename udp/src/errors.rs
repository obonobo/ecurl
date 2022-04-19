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
pub struct ServerError {
    /// Optional source error
    src: Option<Box<dyn Error>>,
    msg: String,
}

impl ServerError {
    /// An empty ServerError
    pub fn new() -> Self {
        Self {
            src: None,
            msg: String::from(""),
        }
    }

    pub fn msg(self, msg: &str) -> Self {
        Self {
            msg: String::from(msg),
            ..self
        }
    }

    pub fn wrap(self, err: Box<dyn Error>) -> Self {
        Self {
            src: Some(err),
            ..self
        }
    }

    pub fn malformed_request() -> Self {
        Self::wrap_err(MalformedRequestError(None))
    }

    pub fn unsupported_proto() -> Self {
        Self::wrap_err(UnsupportedProtoError(None))
    }

    pub fn unsupported_method() -> Self {
        Self::wrap_err(UnsupportedMethodError(None))
    }

    pub fn writing_to_directory() -> Self {
        Self::wrap_err(WritingToDirectoryError(None))
    }

    pub fn writing_to_symlink() -> Self {
        Self::wrap_err(WritingToSymlinkError(None))
    }

    pub fn wrapping(err: Box<dyn Error>) -> Self {
        let msg = format!("{}: {}", type_name::<Self>(), err);
        Self::new().wrap(err).msg(&msg)
    }

    pub fn wrap_err(err: impl Error + 'static) -> Self {
        Self::wrapping(Box::new(err))
    }
}

impl Default for ServerError {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for ServerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}
impl Error for ServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.src.as_ref()?.as_ref())
    }
}

// Some simple errors
super::basic_error!(MalformedRequestError, "Malformed request");
super::basic_error!(UnsupportedProtoError, "Unsupported protocol");
super::basic_error!(UnsupportedMethodError, "Unsupported HTTP method");
super::basic_error!(WritingToDirectoryError, "File exists and is a directory");
super::basic_error!(WritingToSymlinkError, "File exists and is a symlink");
super::basic_error!(
    UdpxConnectionClosed,
    "Trying to read from a closed UdpxStream"
);

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

pub use self::macros::*;
mod macros {
    /// A macro for generating basic errors containing a fixed string message
    /// with the option to append a custom string to the message when the error
    /// is instantiated
    #[macro_export]
    macro_rules! basic_error {
        ($type:ident, $msg:expr) => {
            #[derive(Debug)]
            pub struct $type(pub Option<String>);
            impl Error for $type {}
            impl Display for $type {
                fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                    let out = $msg;
                    match &self.0 {
                        Some(msg) => write!(f, "{}: {}", out, msg),
                        None => write!(f, "{}", out),
                    }
                }
            }
        };
    }
}
