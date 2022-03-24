#[cfg(test)]
pub mod test_utils;

use crate::test_utils::*;
use httpfs::bullshit_scanner::BullshitScanner;
use std::{io::Write, net::TcpStream, sync::Mutex};

// Global server factory for running tests in parallel
lazy_static::lazy_static! {
    static ref SERVERS: Mutex<AddressCountingServerFactory> = Mutex::new(
        AddressCountingServerFactory::default(),
    );
}

fn server() -> ServerDropper {
    SERVERS.lock().unwrap().next_server()
}

#[test]
fn test_simple_get() {
    let handle = server();
    let contents = "Hello world!\n";
    let file = TempFile::new_or_panic("hello!.txt", contents);
    let got = ureq::get(&handle.file_addr(&file.name))
        .call()
        .unwrap()
        .into_string()
        .unwrap();

    assert_eq!(contents, &got);
}

#[test]
fn test_simple_post() {
    let handle = server();
    let contents = "Hello world!\n";
    let file = TempFile::new_or_panic("hello.txt", "");
    let posted = ureq::post(&handle.file_addr(&file.name))
        .send_string(contents)
        .unwrap();

    // Check that the file was uploaded properly
    assert_eq!(posted.status(), 201);

    // Check the contents of the file
    let got = ureq::get(&handle.file_addr(&file.name))
        .call()
        .unwrap()
        .into_string()
        .unwrap();

    assert_eq!(contents, &got);
}

#[test]
fn test_not_found() {
    let handle = server();
    assertions::assert_request_returns_error(
        ureq::get(&handle.file_addr("hello.txt")),
        404,
        Some("File '/hello.txt' could not be found on the server (directory being served is ./)\n"),
    );
}

/// Tests that attempting to access files outside the served directory fails
#[test]
fn test_forbidden() {
    // We will make this request with raw TCP because ureq does not like sending
    // invalid URLs like http://localhost:8080/../../somefile.txt

    let handle = server();
    let request = "GET /../../hello.txt HTTP/1.1\r\n\r\n";
    let mut sock = TcpStream::connect(handle.addr().trim_start_matches("http://")).unwrap();
    sock.write_all(request.as_bytes()).unwrap();
    let mut scnr = BullshitScanner::new(&mut sock);

    // Read status line
    let status = scnr
        .next_line()
        .unwrap()
        .0
        .split_once(" ")
        .map(|pair| String::from(pair.1))
        .unwrap();

    assert_eq!("403 Forbidden", status);

    // Read body
    let body = scnr
        .lines()
        .map(|l| l.0)
        .skip_while(|line| line != "") // Skip headers
        .collect::<Vec<_>>()
        .join("\n");

    assert!(body.contains("hello.txt' is located outside the directory that is being served"))
}
