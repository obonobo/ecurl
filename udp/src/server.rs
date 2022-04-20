use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Read,
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Barrier, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use threadpool::ThreadPool;

use crate::{
    bullshit_scanner::BullshitScanner,
    errors::ServerError,
    html::Templater,
    parse::{parse_http_request, Method, Request},
    trait_alias,
    transport::UdpxListener,
    Bindable, Incoming, Listener, Stream,
};

trait_alias! {
    /// A combination of [Send] with a `'static` lifetime
    pub trait Threadsafe = Send + 'static;

    /// A [Stream] that can be passed between threads
    pub trait ThreadsafeStream = Stream + Threadsafe + Templater;

    /// A [Listener] that emits [ThreadsafeStreams](ThreadsafeStream) and is
    /// itself threadsafe
    pub trait ThreadsafeListener = Listener<ThreadsafeStream> + Threadsafe;

    /// A [Bindable] that emits [ThreadsafeListeners](ThreadsafeListener) and is
    /// itself threadsafe
    pub trait ThreadsafeBindable = Bindable<ThreadsafeStream> + Threadsafe;
}

/// 1MB
pub const BUFSIZE: usize = 1 << 20;

/// A config for running the file server.
pub struct Server {
    pub addr: IpAddr,
    pub port: u32,
    pub dir: String,
    pub n_workers: usize,
}

impl Server {
    pub const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    pub const DEFAULT_PORT: u32 = 8080;
    pub const DEFAULT_DIR: &'static str = "./";
    pub const DEFAULT_NUM_THREADS: usize = 4;

    pub fn serve_udpx_with_proxy(
        &self,
        proxy: Option<SocketAddrV4>,
    ) -> Result<Handle, ServerError> {
        ServerRunner {
            addr: self.addr,
            dir: self.dir.clone(),
            port: self.port,
            threads: Arc::new(Mutex::new(ThreadPool::new(self.n_workers))),
        }
        .serve_with_proxy(proxy)
    }

    /// Spins up a server with the configuration specified by the [Server]
    /// struct. This method may be called multiple times to produce multiple
    /// server instances.
    ///
    /// Specify which kind of [Listener] you want to use to service requests.
    /// Possible values for this type are
    /// [UdpxListener](crate::transport::UdpxListener) or
    /// [std::net::tcp::TcpListener]
    pub fn serve<S, L, B>(&self) -> Result<Handle, ServerError>
    where
        S: ThreadsafeStream,
        L: ThreadsafeListener<S>,
        B: ThreadsafeBindable<S>,
    {
        ServerRunner {
            addr: self.addr,
            dir: self.dir.clone(),
            port: self.port,
            threads: Arc::new(Mutex::new(ThreadPool::new(self.n_workers))),
        }
        .serve::<S, L, B>()
    }
}

impl Default for Server {
    fn default() -> Self {
        Self {
            addr: Self::LOCALHOST,
            port: Self::DEFAULT_PORT,
            dir: String::from(Self::DEFAULT_DIR),
            n_workers: Self::DEFAULT_NUM_THREADS,
        }
    }
}

/// Represents a running [Server] that can be shutdown
#[derive(Debug)]
pub struct Handle {
    /// The [ServerRunner] thread will poll this shared variable in between
    /// accepting connections. If the value contained within the [mutex](Mutex)
    /// is true, then the server thread will stop accepting requests.
    exit: Arc<AtomicBool>,
    done: Arc<Barrier>,
    main: Option<JoinHandle<()>>,
    local_addr: SocketAddr,
}

impl Handle {
    pub fn new(local_addr: SocketAddr) -> Self {
        Self {
            exit: Arc::new(AtomicBool::new(false)),
            done: Arc::new(Barrier::new(2)),
            main: None,
            local_addr,
        }
    }

    /// Gracefully shutdown the server
    pub fn shutdown(&mut self) {
        self.exit.store(true, Ordering::SeqCst);
        self.done.wait();
    }

    /// Waits on the main thread contained within this handle
    pub fn join(self) {
        if let Some(main) = self.main {
            main.join().unwrap();
        }
    }

    fn set_main(&mut self, handle: JoinHandle<()>) {
        self.main = Some(handle);
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

impl Default for Handle {
    fn default() -> Self {
        Self::new(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(127, 0, 0, 0),
            0,
        )))
    }
}

impl Clone for Handle {
    /// Note: this does not clone the main thread of the handle, which is not
    /// clonable. Only the original handle may control its main thread. Cloneed
    /// versions of the handle can still remotely shutdown the main thread, but
    /// only the original handle can call [Handle::join]
    fn clone(&self) -> Self {
        Self {
            exit: self.exit.clone(),
            done: self.done.clone(),
            main: None, // We clone everything except the main thread JoinHandle
            ..*self
        }
    }
}

/// The [ServerRunner] is the object that actually initiates the request
/// handling thread. It is mod-private, the only way to instantiate it is
/// through the [Server] public struct.
#[derive(Debug)]
struct ServerRunner {
    addr: IpAddr,
    port: u32,
    dir: String,
    threads: Arc<Mutex<ThreadPool>>,
}

impl ServerRunner {
    fn serve_with_proxy(&mut self, proxy: Option<SocketAddrV4>) -> Result<Handle, ServerError> {
        let addr = self.addr_str();
        log::debug!("Attempting to bind addr {}", addr);

        let mut listener = UdpxListener::bind_with_proxy(addr, proxy).map_err(wrap)?;
        let local_addr = listener.local_addr().map_err(wrap)?;
        log::info!("Starting server on {}", local_addr);
        listener
            .set_nonblocking(true)
            .map_err(ServerError::wrap_err)?;

        let mut handle = Handle::new(local_addr);

        // Spin up a request handler loop in a new thread
        let (handlec, threadsc, dirc) = (handle.clone(), self.threads.clone(), self.dir.clone());
        handle.set_main(thread::spawn(move || {
            for stream in listener.incoming() {
                let stream = match stream {
                    Ok(stream) => stream,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        log::debug!("Accept would block...");

                        // Poll the handle exit flag
                        if handlec.exit.load(Ordering::SeqCst) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                    Err(_) => break,
                };

                log::debug!(
                    "Connection established with {}",
                    stream
                        .peer_addr()
                        .ok()
                        .map(|addr| format!("{}", addr))
                        .unwrap_or_else(|| String::from("..."))
                );

                let dir = dirc.clone();
                threadsc.lock().unwrap().execute(move || {
                    let mut stream = stream;
                    match handle_connection(&mut stream, &dir) {
                        Ok(_) => {}
                        Err(e) => {
                            log::error!("Server.handle_connection(): {}", e);
                            // write_500(&mut stream, &format!("{}", e));
                        }
                    };
                    log::debug!("Server: shutting down stream ({})", stream);
                    // thread::sleep(Duration::from_millis(50));
                    match stream.shutdown() {
                        Ok(_) => log::debug!("Shutdown successful"),
                        Err(e) => log::error!("Shutdown failed: {}", e),
                    };
                    log::debug!("Exiting thread")
                })
            }

            // Join the request threads
            threadsc.lock().unwrap().join();
            handlec.done.wait();
        }));
        Ok(handle)
    }

    fn serve<S, L, B>(&mut self) -> Result<Handle, ServerError>
    where
        S: ThreadsafeStream,
        L: ThreadsafeListener<S>,
        B: ThreadsafeBindable<S>,
    {
        let addr = self.addr_str();
        log::debug!("Attempting to bind addr {}", addr);
        let mut listener = B::bind(addr).map_err(wrap)?;
        let local_addr = listener.local_addr().map_err(wrap)?;
        log::info!("Starting server on {}", local_addr);
        listener
            .set_nonblocking(true)
            .map_err(ServerError::wrap_err)?;

        let mut handle = Handle::new(local_addr);

        // Spin up a request handler loop in a new thread
        let (handlec, threadsc, dirc) = (handle.clone(), self.threads.clone(), self.dir.clone());
        handle.set_main(thread::spawn(move || {
            for stream in listener.incoming() {
                let stream = match stream {
                    Ok(stream) => stream,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // Poll the handle exit flag
                        if handlec.exit.load(Ordering::SeqCst) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(_) => break,
                };

                log::debug!(
                    "Connection established with {}",
                    stream
                        .peer_addr()
                        .ok()
                        .map(|addr| format!("{}", addr))
                        .unwrap_or_else(|| String::from("..."))
                );

                let dir = dirc.clone();
                threadsc.lock().unwrap().execute(move || {
                    let mut stream = stream;
                    match handle_connection(&mut stream, &dir) {
                        Ok(_) => {}
                        Err(e) => {
                            log::error!("Server.handle_connection(): {}", e);
                            // write_500(&mut stream, &format!("{}", e));
                        }
                    };
                })
            }

            // Join the request threads
            threadsc.lock().unwrap().join();
            handlec.done.wait();
        }));
        Ok(handle)
    }

    fn addr_str(&self) -> String {
        format!("{}:{}", self.addr, self.port)
    }
}

/// Routes requests to the appropriate handler
fn handle_connection<S: ThreadsafeStream>(stream: &mut S, dir: &str) -> Result<(), ServerError> {
    // let mut reader = BufReader::with_capacity(BUFSIZE, stream.as_ref());

    // TODO: DEBUG
    // let chugged = stream.borrow_chug().map_err(ServerError::wrap_err)?;
    // let mut reader = chugged.as_bytes();
    // let scnr = BullshitScanner::new(&mut reader).ignoring_eof();
    // TODO: DEBUG
    let scnr = BullshitScanner::new(stream).ignoring_eof();

    let mut req = parse_http_request(scnr)?;
    log::info!("Here is the parsed request: {}", req);

    let filename = req.file.as_str();
    match Requested::parse(dir, &req) {
        Requested::Dir(file) => write_dir_listing(stream, &file),
        Requested::File(file) => match open_file(&file) {
            Ok((name, fh)) => write_file(stream, fh, &name),
            Err(_) => write_404(stream, filename, dir),
        },
        Requested::Upload(filename) => {
            accept_file_upload(&filename, &mut req.body)?;
            write_response::<File, S>(stream, "201 Created", 0, "", None)
        }
        Requested::None => write_404(stream, filename, dir),
        Requested::NotAllowed(filename) => write_not_allowed(stream, &filename, dir),
    }
}

/// Represents the file server operation that the user is requesting
enum Requested {
    Dir(String),
    File(String),
    Upload(String),
    NotAllowed(String),
    None,
}

impl Requested {
    fn parse<R: Read>(dir: &str, req: &Request<R>) -> Requested {
        let dir = Path::new(dir)
            .canonicalize()
            .ok()
            .unwrap_or_else(|| PathBuf::from(dir));

        let file = dir.join(req.file.trim_start_matches('/'));
        let file = file
            .canonicalize()
            .ok()
            .unwrap_or(file)
            .to_string_lossy()
            .to_string();

        log::debug!("Computed request file path: '{}'", file);

        // Check if the user is allowed to access this file (for either reading
        // or writing)
        if Self::file_not_allowed(&file, &dir.to_string_lossy()) {
            return Self::NotAllowed(file);
        }

        match req.method {
            Method::POST => Self::Upload(file),
            Method::Unsupported => Self::None,
            Method::GET => {
                let p = Path::new(&file);
                if p.is_dir() {
                    Self::Dir(file)
                } else if p.is_file() {
                    Self::File(file)
                } else {
                    Self::None
                }
            }
        }
    }

    /// Returns `true` if this file is located outside the dir being served,
    /// `false` otherwise
    fn file_not_allowed(file: &str, dir: &str) -> bool {
        let mut collect = Vec::with_capacity(64);
        for segment in file.split('/') {
            match segment {
                "" | "." => continue,
                ".." => {
                    if collect.len() > 1 {
                        collect.pop();
                    }
                }
                segment => collect.push(segment),
            }
        }
        let file = format!("/{}", collect.join("/"));
        !file.starts_with(dir)
    }
}

/// Saves the given file with the provided file name
fn accept_file_upload(filename: &str, body: &mut dyn Read) -> Result<(), ServerError> {
    let path = Path::new(filename);
    if path.is_dir() {
        return Err(ServerError::writing_to_directory());
    } else if path.is_symlink() {
        return Err(ServerError::writing_to_symlink());
    }

    // const TRUNCATE_ENABLED: bool = false;
    const TRUNCATE_ENABLED: bool = true;
    let mut fh = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(TRUNCATE_ENABLED)
        .open(filename)
        .map_err(wrap)?;

    std::io::copy(body, &mut fh).map(|_| ()).map_err(wrap)
}

fn write_dir_listing<S: ThreadsafeStream>(stream: &mut S, dir: &str) -> Result<(), ServerError> {
    log::debug!("Listing directory {}", dir);

    // Gather a list of files and inject it into the template
    let template = stream.template(
        fs::read_dir(dir)
            .map_err(wrap)?
            .flat_map(Result::ok)
            .map(|file| (file.file_type(), file))
            .filter(|(ft, _)| ft.as_ref().map(|t| !t.is_symlink()).unwrap_or(false))
            .map(|(ft, f)| {
                (
                    ft.map(|x| x.is_dir()).unwrap_or(false),
                    String::from(f.file_name().to_string_lossy()),
                )
            })
            .map(|(ft, f)| if ft { format!("{}/", f) } else { f }),
    );

    write_response(
        stream,
        "200 OK",
        template.len().try_into().map_err(wrap)?,
        "text/html",
        Some(&mut template.as_bytes()),
    )
}

fn open_file(file: &str) -> Result<(String, File), ServerError> {
    let fh = File::open(file).map_err(wrap)?;
    log::debug!("Opening file {}", file);
    Ok((String::from(file), fh))
}

fn write_response_with_headers<S: Stream>(
    stream: &mut S,
    status: &str,
    body_length: u64,
    headers: Option<HashMap<&str, &str>>,
    body: Option<&mut impl Read>,
) -> Result<(), ServerError> {
    let headers = headers.unwrap_or_default();
    log::debug!(
        "Writing response {}, length {}, headers {:?}",
        status,
        body_length,
        headers
    );

    let mut out = vec![format!("HTTP/1.1 {}", status)];

    if !headers.contains_key("Content-Length") {
        out.push(format!("Content-Length: {}", body_length));
    }

    for (key, value) in headers.iter() {
        out.push(format!("{}: {}", key, value));
    }

    out.push(String::from(""));
    out.push(String::from(""));
    let out = out.join("\r\n");

    stream.write(out.as_bytes()).map_err(wrap)?;
    stream.flush().map_err(wrap)?;

    match body {
        Some(body) => {
            std::io::copy(body, stream).map_err(wrap)?;
            stream.flush().map_err(wrap)
        }
        None => Ok(()),
    }
}

/// Writes a response to the stream
fn write_response<R: Read, S: Stream>(
    stream: &mut S,
    status: &str,
    body_length: u64,
    content_type: &str,
    body: Option<&mut R>,
) -> Result<(), ServerError> {
    write_response_with_headers(
        stream,
        status,
        body_length,
        Some(HashMap::from([("Content-Type", content_type)])),
        body,
    )
}

fn wrap<E: std::error::Error + 'static>(err: E) -> ServerError {
    ServerError::wrap_err(err)
}

/// Writes a file response
fn write_file<S: Stream>(stream: &mut S, mut fh: File, filename: &str) -> Result<(), ServerError> {
    write_response_with_headers(
        stream,
        "200 OK",
        fh.metadata().map_err(wrap)?.len(),
        Some(HashMap::from([
            ("Content-Type", parse_mimetype(filename).as_str()),
            (
                "Content-Disposition",
                &format!(
                    r#"attachment; filename="{}""#,
                    filename.split('/').last().unwrap_or(filename)
                ),
            ),
        ])),
        Some(&mut fh),
    )
}

#[allow(dead_code)]
fn write_500<S: Stream>(stream: &mut S, msg: &str) {
    if let Err(e) = write_response(
        stream,
        "500 Internal Server Error",
        msg.len().try_into().unwrap_or(0),
        "text/plain",
        Some(&mut msg.as_bytes()),
    ) {
        log::debug!("{}", e);
    };
}

/// Writes a '404 Not Found' response
fn write_404<S: Stream>(stream: &mut S, filename: &str, dir: &str) -> Result<(), ServerError> {
    let body = format!(
        "File '{}' could not be found on the server (directory being served is {})\n",
        filename, dir
    );

    write_response(
        stream,
        "404 Not Found",
        body.len().try_into().map_err(|e| {
            ServerError::new()
                .msg("bad numerical conversion")
                .wrap(Box::new(e))
        })?,
        "text/plain",
        Some(&mut body.as_bytes()),
    )
}

fn abs_path(file: &str) -> String {
    Path::new(file)
        .canonicalize()
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from(file))
}

fn write_not_allowed<S: Stream>(
    stream: &mut S,
    filename: &str,
    dir: &str,
) -> Result<(), ServerError> {
    let body = format!(
        concat!(
            "File '{}' is located outside the directory that is being served\r\n\r\n",
            "Only files in directory '{}' may be accessed\r\n"
        ),
        abs_path(filename),
        abs_path(dir)
    );

    write_response(
        stream,
        "403 Forbidden",
        body.len().try_into().map_err(|e| {
            ServerError::new()
                .msg("bad numerical conversion")
                .wrap(Box::new(e))
        })?,
        "text/plain",
        Some(&mut body.as_bytes()),
    )
}

/// Parses the mime type from a non-exhaustive list
fn parse_mimetype(filename: &str) -> String {
    match filename.split('/').last().unwrap_or(filename) {
        "Makefile" => mime::TEXT_PLAIN,
        other => match other.split('.').last() {
            Some(x) => match x {
                "png" => mime::IMAGE_PNG,
                "jpg" => mime::IMAGE_JPEG,
                "txt" => mime::TEXT_PLAIN,
                "js" => mime::APPLICATION_JAVASCRIPT,
                "css" => mime::TEXT_CSS,
                "xml" => mime::TEXT_XML,
                "json" => mime::APPLICATION_JSON,
                "html" => mime::TEXT_HTML,
                "pdf" => mime::APPLICATION_PDF,
                "gitignore" => mime::TEXT_PLAIN,
                "lock" => mime::TEXT_PLAIN,
                "md" => return String::from("text/markdown"),
                "toml" => return String::from("application/toml"),
                _ => mime::TEXT_PLAIN,
            },
            None => mime::APPLICATION_OCTET_STREAM,
        },
    }
    .to_string()
}
