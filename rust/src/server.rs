use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Write},
    net::{IpAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Barrier, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use threadpool::ThreadPool;

use crate::{
    bullshit_scanner::BullshitScanner,
    errors::ServerError,
    parse::{parse_http_request, Method, Request},
};

/// 1MB
pub const BUFSIZE: usize = 1 << 20;

const BAD_NUMERICAL_CONVERSION: &str = "bad numerical conversion";

pub struct Server {
    pub addr: IpAddr,
    pub port: i32,
    pub dir: String,
    pub n_workers: usize,
}

impl Server {
    pub fn serve(self) -> Result<Handle, ServerError> {
        ServerRunner {
            addr: self.addr,
            dir: self.dir,
            port: self.port,
            threads: Arc::new(Mutex::new(ThreadPool::new(self.n_workers))),
        }
        .serve()
    }
}

/// Represents a running [Server] that can be shutdown
#[derive(Debug)]
pub struct Handle {
    /// The [ServerRunner] thread will poll this shared variable in between
    /// accepting connections. If the value contained within the [mutex](Mutex)
    /// is true, then the server thread will stop accepting requests.
    exit: Arc<Mutex<bool>>,
    done: Arc<Barrier>,

    main: Option<JoinHandle<()>>,
}

impl Handle {
    pub fn new() -> Self {
        Self {
            exit: Arc::new(Mutex::new(false)),
            done: Arc::new(Barrier::new(2)),
            main: None,
        }
    }

    /// Gracefully shutdown the server
    pub fn shutdown(&mut self) {
        {
            let mut exit = self.exit.lock().unwrap();
            *exit = true;
        }
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
}

impl Clone for Handle {
    /// Note: this does not clone the main thread of the handle, which is not
    /// clonable. Only the original handle may control its main thread.
    fn clone(&self) -> Self {
        Self {
            exit: self.exit.clone(),
            done: self.done.clone(),
            main: None,
        }
    }
}

#[derive(Debug)]
struct ServerRunner {
    addr: IpAddr,
    port: i32,
    dir: String,
    threads: Arc<Mutex<ThreadPool>>,
}

impl ServerRunner {
    fn serve(&self) -> Result<Handle, ServerError> {
        let addr = self.addr_str();
        log::info!("Starting server on {}", addr);

        let listener = TcpListener::bind(addr).map_err(wrap)?;
        listener
            .set_nonblocking(true)
            .map_err(ServerError::wrap_err)?;

        let mut handle = Handle::new();

        // Spin up a request handler loop in a new thread
        let (handlec, threadsc, dirc) = (handle.clone(), self.threads.clone(), self.dir.clone());
        let main_handle = thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(stream) => stream,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // Poll the handle
                        {
                            if *handlec.exit.lock().unwrap() {
                                break;
                            }
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
                        .unwrap_or(String::from("..."))
                );

                let dir = dirc.clone();
                threadsc.lock().unwrap().execute(move || {
                    match handle_connection(&mut stream, &dir) {
                        Ok(_) => {}
                        Err(e) => {
                            log::info!("{}", e);
                            write_500(&mut stream, &format!("{}", e));
                        }
                    };
                })
            }

            // Join the request threads
            threadsc.lock().unwrap().join();
            handlec.done.wait();
        });

        handle.set_main(main_handle);
        Ok(handle)
    }

    fn addr_str(&self) -> String {
        format!("{}:{}", self.addr, self.port)
    }
}

/// Routes requests to the appropriate handler
fn handle_connection(stream: &mut TcpStream, dir: &str) -> Result<(), ServerError> {
    // let mut reader = BufReader::with_capacity(BUFSIZE, stream.as_ref());
    let scnr = BullshitScanner::new(stream);
    let mut req = parse_http_request(scnr)?;
    log::info!("{}", req);

    let filename = req.file.as_str();
    match Requested::parse(dir, &req) {
        Requested::Dir(file) => write_dir_listing(stream, &file),
        Requested::File(file) => match open_file(&file) {
            Ok((name, fh)) => write_file(stream, fh, &name),
            Err(_) => write_404(stream, filename, dir),
        },
        Requested::Upload(filename) => {
            accept_file_upload(&filename, &mut req.body)?;
            write_response::<File>(stream, "201 Created", 0, "", None)
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
        let file = Path::new(dir)
            .join(req.file.trim_start_matches("/"))
            .to_string_lossy()
            .to_string();

        log::debug!("Computed request file path: '{}'", file);

        // Check if the user is allowed to access this file (for either reading
        // or writing)
        if Self::file_not_allowed(&file, &dir) {
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
        let path_buf = |file| {
            Path::new(file)
                .canonicalize()
                .ok()
                .unwrap_or_else(|| PathBuf::from(file))
        };

        let dir = path_buf(dir);
        let file = path_buf(file);

        log::debug!("Security - checking if file is within served folder");
        log::debug!("Dir: {}", dir.to_string_lossy());
        log::debug!("File: {}", file.to_string_lossy());

        !file.starts_with(dir)
    }
}

/// Saves the given file with the provided file name
fn accept_file_upload<'a>(filename: &str, body: &'a mut dyn Read) -> Result<(), ServerError> {
    let mut fh = File::create(filename).map_err(wrap)?;
    std::io::copy(body, &mut fh).map(|_| ()).map_err(wrap)
}

fn write_dir_listing(stream: &mut TcpStream, dir: &str) -> Result<(), ServerError> {
    use std::fmt::Write;

    log::debug!("Listing directory {}", dir);

    let paths = fs::read_dir(dir).map_err(wrap)?;
    let mut out = String::new();

    for p in paths {
        let p = match p {
            Ok(p) => p,
            Err(_) => continue,
        };

        let pp = match p.file_type() {
            Ok(p) => p,
            Err(_) => continue,
        };

        out.write_str(
            if pp.is_dir() {
                format!("{}/\n", p.file_name().to_string_lossy())
            } else {
                format!("{}\n", p.file_name().to_string_lossy())
            }
            .as_str(),
        )
        .map_err(wrap)?;
    }

    write_response(
        stream,
        "200 OK",
        out.len().try_into().map_err(wrap)?,
        "text/plain",
        Some(&mut stringreader::StringReader::new(out.as_str())),
    )
}

fn open_file(file: &str) -> Result<(String, File), ServerError> {
    let fh = File::open(file).map_err(wrap)?;
    log::debug!("Opening file {}", file);
    Ok((String::from(file), fh))
}

fn write_response_with_headers(
    stream: &mut TcpStream,
    status: &str,
    body_length: u64,
    headers: Option<HashMap<&str, &str>>,
    body: Option<&mut impl Read>,
) -> Result<(), ServerError> {
    let headers = headers.unwrap_or_else(HashMap::new);
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
fn write_response<R: Read>(
    stream: &mut TcpStream,
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
fn write_file(stream: &mut TcpStream, mut fh: File, filename: &str) -> Result<(), ServerError> {
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
                    filename.split("/").last().unwrap_or(filename)
                ),
            ),
        ])),
        Some(&mut fh),
    )
}

fn write_500(stream: &mut TcpStream, msg: &str) {
    if let Err(e) = write_response(
        stream,
        "500 Internal Server Error",
        msg.len().try_into().unwrap_or(0),
        "text/plain",
        Some(&mut stringreader::StringReader::new(msg)),
    ) {
        log::debug!("{}", e);
    };
}

/// Writes a '404 Not Found' response
fn write_404(stream: &mut TcpStream, filename: &str, dir: &str) -> Result<(), ServerError> {
    let body = format!(
        "File '{}' could not be found on the server (directory being served is {})",
        filename, dir
    );

    write_response(
        stream,
        "404 Not Found",
        body.len().try_into().map_err(|e| {
            ServerError::new()
                .msg(BAD_NUMERICAL_CONVERSION)
                .wrap(Box::new(e))
        })?,
        "text/plain",
        Some(&mut stringreader::StringReader::new(body.as_str())),
    )
}

fn abs_path(file: &str) -> String {
    Path::new(file)
        .canonicalize()
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(String::from(file))
}

fn write_not_allowed(stream: &mut TcpStream, filename: &str, dir: &str) -> Result<(), ServerError> {
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
                .msg(BAD_NUMERICAL_CONVERSION)
                .wrap(Box::new(e))
        })?,
        "text/plain",
        Some(&mut stringreader::StringReader::new(body.as_str())),
    )
}

/// Parses the mime type from a non-exhaustive list
fn parse_mimetype(filename: &str) -> String {
    match filename.split("/").last().unwrap_or(filename) {
        "Makefile" => mime::TEXT_PLAIN,
        other => match other.split(".").last() {
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
                "toml" => return String::from("application/toml"),
                _ => mime::APPLICATION_OCTET_STREAM,
            },
            None => mime::APPLICATION_OCTET_STREAM,
        },
    }
    .to_string()
}
