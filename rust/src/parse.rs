use std::{
    fmt::{Debug, Display},
    io::Read,
    str,
};

use crate::errors::ServerError;

/// HTTP request methods
#[derive(Debug)]
pub enum Method {
    GET,
    POST,

    /// Represents an request with an unsupported HTTP method
    Unsupported,
}

impl Method {
    pub fn from(string: &str) -> Self {
        match string.to_lowercase().as_str() {
            "get" => Method::GET,
            _ => Method::Unsupported,
        }
    }
}

impl Default for Method {
    fn default() -> Self {
        Method::Unsupported
    }
}

#[derive(Debug)]
pub enum Proto {
    HTTP1_1,
    HTTP1_0,
    Unsupported,
}

impl Proto {
    pub fn from(string: &str) -> Self {
        match string.to_lowercase().as_str() {
            "http/1.1" => Proto::HTTP1_1,
            "http/1.0" => Proto::HTTP1_0,
            _ => Proto::Unsupported,
        }
    }
}

impl Default for Proto {
    fn default() -> Self {
        Proto::HTTP1_1
    }
}

#[derive(Default)]
pub struct Request {
    pub proto: Proto,
    pub method: Method,
    pub file: String,
    pub body: Option<Box<dyn Read>>,
}

impl Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("proto", &self.proto)
            .field("method", &self.method)
            .field("file", &self.file)
            .field(
                "body",
                &match self.body {
                    Some(_) => format!("..."),
                    None => format!("n/a"),
                },
            )
            .finish()
    }
}

impl Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub fn parse_http_request(reader: impl Read) -> Result<Request, ServerError> {
    let (proto, method, file) = parse_request_line(&reader)?;

    Ok(Request {
        proto,
        method,
        file,
        body: Some(parse_body(reader)?),
    })
}

fn parse_body(reader: impl Read) -> Result<Box<dyn Read>, ServerError> {
    Ok(Box::new(stringreader::StringReader::new("")))
}

fn parse_request_line(reader: &impl Read) -> Result<(Proto, Method, String), ServerError> {
    let (line, _) = read_line(reader)?;
    let words = line.split_whitespace().collect::<Vec<_>>();

    let map_err =
        |word| ServerError::malformed_request(format!("no {} found in request line", word));

    Ok((
        (match words.get(2) /* PROTO */ {
            Some(proto) => match Proto::from(*proto) {
                Proto::Unsupported => Err(ServerError::unsupported_proto(*proto)),
                proto => Ok(proto),
            },
            None => Err(map_err("protocol")),
        })?,
        (match words.get(0) /* METHOD */ {
            Some(method) => match Method::from(*method) {
                Method::Unsupported => Err(ServerError::unsupported_method(*method)),
                method => Ok(method),
            },
            None => Err(map_err("method")),
        })?,
        (match words.get(1) /* PATH */ {
            Some(path) => Ok(String::from(*path)),
            None => Err(map_err("path")),
        })?,
    ))
}

/// Reads a single line from the reader. Returns the line and how many bytes
/// were read to obtain that line. Trailing '\r' and '\n' are removed.
fn read_line(reader: &impl Read) -> Result<(String, usize), ServerError> {
    let mut n = 0;
    let mut s = Vec::with_capacity(1024);
    for b in reader.bytes() {
        let b = b.map_err(ServerError::malformed_request)?;
        n += 1;
        if b == b'\n' {
            break;
        }
        s.push(b);
    }
    Ok((
        String::from(
            str::from_utf8(&s)
                .map_err(ServerError::malformed_request)?
                .trim_end_matches("\r"),
        ),
        n,
    ))
}
