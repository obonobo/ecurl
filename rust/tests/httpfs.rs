#[cfg(test)]
pub mod test_utils;

use crate::test_utils::*;
use core::panic;
use httpfs::bullshit_scanner::BullshitScanner;
use std::{
    io::Write,
    net::TcpStream,
    sync::{mpsc, Arc, Mutex},
    thread,
};
use test_utils::better_ureq::*;

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

/// Tests multiple clients reading the same file
#[test]
fn test_multiple_clients_get_same_file() {
    let server = server();
    let contents = "Hello world\n";
    let file = TempFile::new("hello.txt", contents).unwrap();
    let n = 25;
    let mut threads = Vec::with_capacity(n);
    let (taskout, taskin) = mpsc::channel::<Result<(u16, String), ureq::Error>>();
    let addr = server.file_addr(&file.name);

    // Spawn some clients
    for _ in 0..n {
        let (out, addr) = (taskout.clone(), addr.clone());
        threads.push(thread::spawn(move || {
            out.send(ureq_get_errors_are_ok(&addr)).unwrap()
        }));
    }

    // Assert client results
    threads.into_iter().for_each(|t| t.join().unwrap());
    for (i, res) in taskin.iter().take(n).enumerate() {
        match res {
            Ok((code, body)) => {
                assert_eq!(200, code);
                assert_eq!(contents, body);
            }
            Err(e) => panic!("Got an error on request {}: {}", i, e),
        }
    }
}

/// Tests multiple clients reading and writing the same file
#[test]
fn test_multiple_clients_reading_and_writing_same_file() {
    let handle = server();
    let contents = "Hello world\n";
    let file = TempFile::new("hello.txt", contents).unwrap();

    let n = 25;
    let mut threads = Vec::with_capacity(n);
    let (taskout, taskin) = mpsc::channel::<Result<(u16, String), ureq::Error>>();
    let addr = handle.file_addr(&file.name);

    // The task function will be different for each thread. We will alternate
    // between one thread reading, one thread writing, one reading, one writing,
    // etc.
    let mut read = 0;
    let mut task =
        || -> Arc<dyn Fn(&str, &str) -> Result<(u16, String), ureq::Error> + Send + Sync> {
            read += 1;
            Arc::new(if read % 2 == 0 {
                |path, _| ureq_get_errors_are_ok(path)
            } else {
                |path, body| ureq_post_errors_are_ok(path, body)
            })
        };

    // Spawn the clients, some will read, some will write
    for i in 0..n {
        let (out, path, task) = (taskout.clone(), addr.clone(), task());
        let body = format!("From thread {}", i);
        threads.push(thread::spawn(move || out.send(task(&path, &body)).unwrap()));
    }

    // Assert client results
    threads.into_iter().for_each(|t| t.join().unwrap());
    let results = taskin.iter().take(n).collect::<Vec<_>>();
    for (i, res) in results.iter().enumerate() {
        match res {
            Ok((code, body)) => match code {
                200 => assert!(
                    body.contains("From thread") || body.contains(contents),
                    "Body: {}",
                    body
                ),
                201 => assert_eq!("", body),
                code => panic!("Expected status 200 or 201 but got {}", code),
            },
            Err(e) => panic!("Got an error on request {}: {}", i, e),
        }
    }
}
