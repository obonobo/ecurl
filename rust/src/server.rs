use std::{
    fs::{self, File},
    io::{BufReader, Read, Write},
    net::{IpAddr, TcpListener, TcpStream},
    path::Path,
};

use threadpool::ThreadPool;

use crate::{
    errors::ServerError,
    parse::{parse_http_request, Method, Request},
};

/// 1MB
pub const BUFSIZE: usize = 1 << 20;

pub struct Server {
    pub addr: IpAddr,
    pub port: i32,
    pub dir: String,
    pub n_workers: usize,
}

impl Server {
    pub fn serve(self) -> Result<Handle, ServerError> {
        (ServerRunner {
            addr: self.addr,
            dir: self.dir,
            port: self.port,
            threads: ThreadPool::new(self.n_workers),
        })
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

        let listener = TcpListener::bind(addr).map_err(ServerError::from)?;
        for stream in listener.incoming() {
            let stream = stream.map_err(ServerError::from)?;
            log::debug!(
                "Connection established with {}",
                stream.peer_addr().map_err(ServerError::from)?
            );

            let dir = self.dir.clone();
            self.threads.execute(move || {
                match handle_connection(stream, &dir) {
                    Ok(_) => {}
                    Err(e) => log::info!("{}", e),
                };
            })
        }

        Ok(Handle {})
    }

    fn addr_str(&self) -> String {
        format!("{}:{}", self.addr, self.port)
    }
}

fn handle_connection(stream: TcpStream, dir: &str) -> Result<(), ServerError> {
    let reader = BufReader::with_capacity(BUFSIZE, &stream);
    let req = parse_http_request(reader)?;
    log::info!("{}", req);

    match Requested::parse(dir, req) {
        Requested::Dir(file) => write_dir_listing(stream, &file),
        Requested::File(file) => match open_file(&file) {
            Ok((name, fh)) => write_file(stream, fh, &name),
            Err(_) => write_404(stream),
        },
        Requested::Upload { filename, body } => accept_file_upload(stream, &filename, body),
        Requested::None => write_404(stream),
    }
}

enum Requested {
    Dir(String),
    File(String),
    Upload {
        filename: String,
        body: Box<dyn Read>,
    },
    None,
}

impl Requested {
    fn parse(dir: &str, req: Request) -> Requested {
        let file = Path::new(dir)
            .join(req.file.trim_start_matches("/"))
            .to_string_lossy()
            .to_string();

        match req.method {
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
            Method::POST => Self::Upload {
                filename: file,
                body: req
                    .body
                    .unwrap_or(Box::new(stringreader::StringReader::new(""))),
            },
            Method::Unsupported => Self::None,
        }
    }
}

fn accept_file_upload(
    stream: TcpStream,
    filename: &str,
    body: impl Read,
) -> Result<(), ServerError> {
    Ok(())
}

fn write_dir_listing(stream: TcpStream, dir: &str) -> Result<(), ServerError> {
    use std::fmt::Write;

    log::debug!("Listing directory {}", dir);

    let paths = fs::read_dir(dir).map_err(ServerError::from)?;
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
        .map_err(ServerError::from)?;
    }
    write_response(
        stream,
        "200 OK",
        out.len().try_into().map_err(ServerError::from)?,
        "text/plain",
        Some(&mut stringreader::StringReader::new(out.as_str())),
    )
}

fn open_file(file: &str) -> Result<(String, File), ServerError> {
    let fh = File::open(file).map_err(ServerError::from)?;
    log::debug!("Opening file {}", file);
    Ok((String::from(file), fh))
}

/// Writes a response to the stream
fn write_response<R: Read>(
    mut stream: TcpStream,
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

    stream.write(out.as_bytes()).map_err(ServerError::from)?;
    stream.flush().map_err(ServerError::from)?;

    match body {
        Some(body) => {
            std::io::copy(body, &mut stream).map_err(ServerError::from)?;
            stream.flush().map_err(ServerError::from)
        }
        None => Ok(()),
    }
}

/// Writes a file response
fn write_file(stream: TcpStream, mut fh: File, filename: &str) -> Result<(), ServerError> {
    let metadata = fh.metadata().map_err(ServerError::from)?;
    write_response(
        stream,
        "200 OK",
        metadata.len(),
        parse_mimetype(filename).as_str(),
        Some(&mut fh),
    )
}

/// Writes a NOT FOUND response
fn write_404(stream: TcpStream) -> Result<(), ServerError> {
    write_response::<File>(stream, "400 NOT FOUND", 0, "", None)
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

            // ... and so on, this is where you'd fill out more info ideally
            _ => mime::APPLICATION_OCTET_STREAM,
        },
        None => mime::APPLICATION_OCTET_STREAM,
    }
    .to_string()
}
