use std::{
    fs::{self, File},
    io::{Read, Write},
    net::{IpAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
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
            threads: ThreadPool::new(self.n_workers),
        }
        .serve()
    }
}

#[derive(Debug)]
struct ServerRunner {
    addr: IpAddr,
    port: i32,
    dir: String,
    threads: ThreadPool,
}

pub struct Handle {}

impl ServerRunner {
    pub fn serve(&self) -> Result<Handle, ServerError> {
        let addr = self.addr_str();
        log::info!("Starting server on {}", addr);

        let listener = TcpListener::bind(addr).map_err(wrap)?;
        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(stream) => stream,
                Err(_) => continue,
            };

            if let Some(addr) = stream.peer_addr().ok() {
                log::debug!("Connection established with {}", addr);
            }

            let dir = self.dir.clone();
            self.threads.execute(move || {
                match handle_connection(&mut stream, &dir) {
                    Ok(_) => {}
                    Err(e) => {
                        log::info!("{}", e);
                        write_500(&mut stream, &format!("{}", e));
                    }
                };
            })
        }

        Ok(Handle {})
    }

    fn addr_str(&self) -> String {
        format!("{}:{}", self.addr, self.port)
    }
}

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

/// Writes a response to the stream
fn write_response<R: Read>(
    stream: &mut TcpStream,
    status: &str,
    body_length: u64,
    content_type: &str,
    body: Option<&mut R>,
) -> Result<(), ServerError> {
    log::debug!(
        "Writing response {}, length {}, media type {}",
        status,
        body_length,
        content_type
    );

    let mut out = vec![
        format!("HTTP/1.1 {}", status),
        format!("Content-Length: {}", body_length),
    ];

    if content_type.len() > 0 {
        out.push(format!("Content-Type: {}", content_type));
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

fn wrap<E: std::error::Error + 'static>(err: E) -> ServerError {
    ServerError::wrap_err(err)
}

/// Writes a file response
fn write_file(stream: &mut TcpStream, mut fh: File, filename: &str) -> Result<(), ServerError> {
    write_response(
        stream,
        "200 OK",
        fh.metadata().map_err(wrap)?.len(),
        parse_mimetype(filename).as_str(),
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
    match filename.split(".").last() {
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

            // ... and so on, this is where you'd fill out more info ideally
            _ => mime::APPLICATION_OCTET_STREAM,
        },
        None => mime::APPLICATION_OCTET_STREAM,
    }
    .to_string()
}
