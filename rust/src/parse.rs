use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    io::{Read, Take},
    str,
};

use crate::{
    bullshit_scanner::BullshitScanner,
    errors::{MalformedRequestError, ServerError, UnsupportedMethodError, UnsupportedProtoError},
};

const CONTENT_LENGTH: &str = "Content-Length";

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
            "post" => Method::POST,
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

pub struct Request<R>
where
    R: Read,
{
    pub proto: Proto,
    pub method: Method,
    pub file: String,
    pub headers: HashMap<String, String>,
    pub body: R,
}

impl<R: Read> Debug for Request<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("proto", &self.proto)
            .field("method", &self.method)
            .field("file", &self.file)
            .field("body", &"...")
            .finish()
    }
}

impl<R: Read> Display for Request<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub fn parse_http_request(
    mut scnr: BullshitScanner,
) -> Result<Request<Take<BullshitScanner>>, ServerError> {
    let (proto, method, file) = parse_request_line(&mut scnr)?;
    let headers = parse_headers(&mut scnr)?;
    let limit = headers
        .get(CONTENT_LENGTH)
        .map(|l| l.parse::<u64>().ok().unwrap_or(0))
        .unwrap_or(0);

    Ok(Request {
        proto,
        method,
        file,
        headers,
        body: scnr.take(limit),
    })
}

fn parse_headers(scnr: &mut BullshitScanner) -> Result<HashMap<String, String>, ServerError> {
    // Headers we read line-by-line
    let mut headers = HashMap::with_capacity(64);
    loop {
        let line = scnr.next_line().map(|l| l.0).map_err(|_| {
            ServerError::new().wrap(Box::new(MalformedRequestError(Some(String::from(
                "invalid request headers, headers must end with '\\r\\n'",
            )))))
        })?;

        if &line == "" {
            return Ok(headers);
        }

        let (left, right) = line.split_once(":").ok_or_else(|| {
            ServerError::new().wrap(Box::new(MalformedRequestError(Some(format!(
                "failed to parse request header '{}'",
                line
            )))))
        })?;

        headers.insert(String::from(left.trim()), String::from(right.trim()));
    }
}

fn parse_request_line(scnr: &mut BullshitScanner) -> Result<(Proto, Method, String), ServerError> {
    let words = scnr
        .next_line()
        .map(|l| l.0)
        .map_err(|e| ServerError::new().msg(&format!("{}", e)))?
        .split_whitespace()
        .map(|s| String::from(s))
        .collect::<Vec<_>>();

    let map_err = |word| {
        ServerError::wrapping(Box::new(MalformedRequestError(Some(format!(
            "no {} found in request line",
            word
        )))))
    };

    let proto = (match words.get(2) {
        Some(proto) => match Proto::from(proto) {
            Proto::Unsupported => Err(ServerError::wrapping(Box::new(UnsupportedProtoError(Some(
                String::from(proto),
            ))))),
            proto => Ok(proto),
        },
        None => Err(map_err("protocol")),
    })?;

    let method = (match words.get(0) {
        Some(method) => match Method::from(method) {
            Method::Unsupported => Err(ServerError::wrapping(Box::new(UnsupportedMethodError(Some(
                String::from(method),
            ))))),
            method => Ok(method),
        },
        None => Err(map_err("method")),
    })?;

    let path = (match words.get(1) {
        Some(path) => Ok(String::from(path)),
        None => Err(map_err("path")),
    })?;

    Ok((proto, method, path))
}
