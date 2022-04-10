use std::{
    fs,
    io::{Error, Write},
    net::IpAddr,
};

use httpfs::{
    errors::ServerError,
    server::{Handle, Server},
};

use rand::{distributions::Alphanumeric, thread_rng, Rng};

pub type ServerConfig = (IpAddr, u32, &'static str, usize);

/// When [dropped](Drop), the [TempFile] gets deleted.
pub struct TempFile {
    pub name: String,
}

impl TempFile {
    /// Creates a temporary file with the provided contents. To avoid filename
    /// conflicts, the filename will be prefixed with a random string
    pub fn new(filename: &str, contents: &str) -> Result<Self, Error> {
        let filename = vec![
            "TEMP_",
            thread_rng()
                .sample_iter(&Alphanumeric)
                .take(16)
                .map(char::from)
                .collect::<String>()
                .as_str(),
            "_",
            filename,
        ]
        .into_iter()
        .collect::<String>();

        fs::File::create(&filename)?.write_all(contents.as_bytes())?;
        Ok(Self { name: filename })
    }

    pub fn new_or_panic(filename: &str, contents: &str) -> Self {
        Self::new(filename, contents).unwrap()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        fs::remove_file(&self.name).unwrap();
    }
}

impl Default for TempFile {
    /// Creates an empty temp file
    fn default() -> Self {
        Self::new_or_panic("file.tmp", "")
    }
}

/// A wrapper around the server handle. Implements a [Drop::drop] method that
/// calls [Handle::shutdown]. Warning: [Handle::shutdown] may block for a short
/// time while it waits for the server to stop. That's the reason why this is
/// not implemented for the general [Server] type.
pub struct ServerDropper {
    handle: Handle,
    cfg: ServerConfig,
}

impl ServerDropper {
    pub const DEFAULT_SERVER_CONFIG: ServerConfig = (Server::LOCALHOST, 8666, "./", 2);

    pub fn new(cfg: ServerConfig) -> Result<Self, ServerError> {
        Ok(Self {
            cfg,
            handle: Server {
                addr: cfg.0,
                port: cfg.1,
                dir: String::from(cfg.2),
                n_workers: cfg.3,
            }
            .serve()?,
        })
    }

    pub fn new_or_panic(cfg: ServerConfig) -> Self {
        Self::new(cfg).unwrap()
    }

    /// Returns a formatted string containing the address of this server
    pub fn addr(&self) -> String {
        format!("http://{}:{}", self.cfg.0, self.cfg.1)
    }

    pub fn file_addr(&self, filename: &str) -> String {
        format!("{}/{}", self.addr(), filename)
    }
}

impl Default for ServerDropper {
    /// Note: this panics if the server cannot be created
    fn default() -> Self {
        Self::new_or_panic(Self::DEFAULT_SERVER_CONFIG)
    }
}

impl Drop for ServerDropper {
    fn drop(&mut self) {
        self.handle.shutdown();
    }
}

/// Spawns [ServerDroppers](ServerDropper) on an auto-incrementing port starting
/// at some provided port number. Used for concurrent tests.
///
/// The way to use this is to make a global singleton that is reused for all
/// your tests.
///
/// ### Examples
///
/// ```
/// lazy_static::lazy_static! {
///     static ref SERVERS: Mutex<AddressCountingServerFactory> = Mutex::new(
///         AddressCountingServerFactory::default(),
///     );
/// }
/// ```
pub struct AddressCountingServerFactory {
    next: u32,
}

impl AddressCountingServerFactory {
    pub fn new(starting_port: u32) -> Self {
        Self {
            next: starting_port,
        }
    }

    pub fn next_server(&mut self) -> ServerDropper {
        let mut cfg = ServerDropper::DEFAULT_SERVER_CONFIG;
        cfg.1 = self.next;
        self.next += 1;
        ServerDropper::new_or_panic(cfg)
    }
}

impl Default for AddressCountingServerFactory {
    fn default() -> Self {
        Self {
            next: ServerDropper::DEFAULT_SERVER_CONFIG.1,
        }
    }
}

pub mod better_ureq {
    use ureq::{get, post, Error};

    /// Calls ureq GET but treats [ureq::Error::Status] errors as still being valid.
    /// Returns a tuple of status code and response body string.
    pub fn ureq_get_errors_are_ok(path: &str) -> Result<(u16, String), Error> {
        ureq_errors_are_ok(|| get(path).call())
    }

    pub fn ureq_post_errors_are_ok(path: &str, body: &str) -> Result<(u16, String), Error> {
        ureq_errors_are_ok(|| post(path).send_string(body))
    }

    fn ureq_errors_are_ok(
        callable: impl FnOnce() -> Result<ureq::Response, Error>,
    ) -> Result<(u16, String), Error> {
        match callable() {
            Ok(response) | Err(Error::Status(_, response)) => Ok((
                response.status(),
                response.into_string().unwrap_or_default(),
            )),
            Err(e) => Err(e),
        }
    }
}
pub mod assertions {
    use ureq::{Error::Status, Request};

    /// Asserts that a given [Request] returns an HTTP error code with a
    /// specific body. Pass [Option::None] if you don't want to assert the body
    pub fn assert_request_returns_error(req: Request, status: u16, body: Option<&str>) {
        match req.call().err().unwrap() {
            Status(code, res) => {
                assert_eq!(status, code,);
                if let Some(body) = body {
                    let actual_body = res.into_string().unwrap();
                    assert_eq!(body, actual_body);
                }
            }
            err => panic!(
                "expected request to return an error status code and a message but got err {}",
                err
            ),
        }
    }
}
