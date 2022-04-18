use std::{
    fmt::Display,
    fs,
    io::{Error, Write},
    net::{IpAddr, TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
};

use udpx::{
    errors::ServerError,
    server::{Handle, Server, ThreadsafeBindable, ThreadsafeListener, ThreadsafeStream},
    transport::{UdpxListener, UdpxStream},
    util::logging::init_logging,
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
        let filename = format!(
            "TEMP_{}_{}",
            thread_rng()
                .sample_iter(&Alphanumeric)
                .take(16)
                .map(char::from)
                .collect::<String>(),
            filename
        );
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
}

impl ServerDropper {
    pub const DEFAULT_SERVER_CONFIG: ServerConfig = (Server::LOCALHOST, 8666, "./", 2);

    pub fn new<S, L, B>(cfg: ServerConfig) -> Result<Self, ServerError>
    where
        S: ThreadsafeStream,
        L: ThreadsafeListener<S>,
        B: ThreadsafeBindable<S>,
    {
        let server = Server {
            addr: cfg.0,
            port: cfg.1,
            dir: String::from(cfg.2),
            n_workers: cfg.3,
        };

        Ok(Self {
            handle: server.serve::<S, L, B>()?,
        })
    }

    pub fn new_or_panic<S, L, B>(cfg: ServerConfig) -> Self
    where
        S: ThreadsafeStream,
        L: ThreadsafeListener<S>,
        B: ThreadsafeBindable<S>,
    {
        Self::new::<S, L, B>(cfg).unwrap()
    }

    pub fn new_random_port<S, L, B>() -> Self
    where
        S: ThreadsafeStream,
        L: ThreadsafeListener<S>,
        B: ThreadsafeBindable<S>,
    {
        todo!()
    }

    /// Starts a [ServerDropper] on a random port. The port is provided by the OS.
    pub fn server<S, L, B>() -> ServerDropper
    where
        S: ThreadsafeStream,
        L: ThreadsafeListener<S>,
        B: ThreadsafeBindable<S>,
    {
        let mut cfg = ServerDropper::DEFAULT_SERVER_CONFIG;
        cfg.1 = 0;
        ServerDropper::new_or_panic::<S, L, B>(cfg)
    }

    pub fn tcpserver() -> ServerDropper {
        Self::server::<TcpStream, TcpListener, TcpListener>()
    }

    pub fn udpxserver() -> ServerDropper {
        Self::server::<UdpxStream, UdpxListener, UdpxListener>()
    }

    /// Returns a formatted string containing the address of this server
    pub fn addr(&self) -> String {
        self.handle.local_addr().to_string()
    }

    pub fn file_addr(&self, filename: &str) -> String {
        let addr = self.addr();
        format!("http://{}/{}", addr, filename)
    }
}

impl Default for ServerDropper {
    /// Note: this panics if the server cannot be created
    fn default() -> Self {
        Self::new_or_panic::<TcpStream, TcpListener, TcpListener>(Self::DEFAULT_SERVER_CONFIG)
    }
}

impl Drop for ServerDropper {
    fn drop(&mut self) {
        self.handle.shutdown();
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

/// A wrapper that let's you print [Results](Result)
pub struct DisplayResult<T, E>(pub Result<T, E>);

impl<T: Display, E: Display> Display for DisplayResult<T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Ok(value) => write!(f, "Ok({})", value),
            Err(value) => write!(f, "Err({})", value),
        }
    }
}

pub static LOGS: LoggingInitializer = LoggingInitializer::new();

pub struct LoggingInitializer {
    initialized: AtomicBool,
}

impl LoggingInitializer {
    pub const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
        }
    }

    pub fn initialize(&self) {
        if !self.initialized.load(Ordering::SeqCst) {
            self.initialized.store(true, Ordering::SeqCst);
            init_logging(true);
        }
    }
}

impl Default for LoggingInitializer {
    fn default() -> Self {
        Self::new()
    }
}

pub mod simple_udpx {
    use std::{net::SocketAddr, sync::mpsc, thread, time::Duration};

    use udpx::{transport::UdpxListener, util, Bindable, Listener};

    /// Spins up a simple UDPx server on a random address using the provided
    /// handler
    pub fn serve<S, R>(handler: S) -> SocketAddr
    where
        S: 'static + Send + FnOnce(UdpxListener) -> R,
    {
        let (addrsend, addrrecv) = mpsc::channel();
        thread::spawn(move || {
            handler(
                UdpxListener::bind("127.0.0.1:0")
                    .and_then(|l| {
                        l.local_addr()
                            .and_then(|a| addrsend.send(a).map_err(util::InTwo::intwo).map(|_| l))
                    })
                    .expect("Send error: server cannot report its address"),
            );
        });

        addrrecv
            .recv_timeout(Duration::from_millis(100))
            .expect("Timed out while waiting for server to report its address")
    }
}
