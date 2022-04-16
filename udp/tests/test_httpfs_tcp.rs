#[cfg(test)]
mod test_utils;

use core::panic;
use std::{
    io::Write,
    net::TcpStream,
    sync::{mpsc, Arc},
    thread,
};
use test_utils::{better_ureq::*, *};
use udpx::bullshit_scanner::BullshitScanner;

#[test]
fn test_simple_get() {
    let handle = ServerDropper::tcpserver();
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
    let handle = ServerDropper::tcpserver();
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
    let handle = ServerDropper::tcpserver();
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

    let handle = ServerDropper::tcpserver();
    let request = "GET /../../hello.txt HTTP/1.1\r\n\r\n";
    let mut sock = TcpStream::connect(handle.addr().trim_start_matches("http://")).unwrap();
    sock.write_all(request.as_bytes()).unwrap();
    let mut scnr = BullshitScanner::new(&mut sock);

    // Read status line
    let status = scnr
        .next_line()
        .unwrap()
        .0
        .split_once(' ')
        .map(|pair| String::from(pair.1))
        .unwrap();

    assert_eq!("403 Forbidden", status);

    // Read body
    let body = scnr
        .lines()
        .map(|l| l.0)
        .skip_while(|line| !line.is_empty()) // Skip headers
        .collect::<Vec<_>>()
        .join("\n");

    assert!(body.contains("hello.txt' is located outside the directory that is being served"))
}

/// Tests multiple clients reading the same file
#[test]
fn test_multiple_clients_get_same_file() {
    let server = ServerDropper::tcpserver();
    let contents = "Hello world\n";
    let file = TempFile::new("hello.txt", contents).unwrap();
    let (taskout, taskin) = mpsc::channel();
    let addr = server.file_addr(&file.name);

    // Spawn some clients
    for _ in 0..25 {
        let (out, addr) = (taskout.clone(), addr.clone());
        thread::spawn(move || out.send(ureq_get_errors_are_ok(&addr)).unwrap());
    }
    drop(taskout);

    // Assert client results
    for (i, res) in taskin.iter().enumerate() {
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
    let handle = ServerDropper::tcpserver();
    let contents = "Hello world\n";
    let file = TempFile::new("hello.txt", contents).unwrap();
    let (taskout, taskin) = mpsc::channel();
    let addr = handle.file_addr(&file.name);

    // The task function will be different for each thread. We will alternate
    // between one thread reading, one thread writing, one reading, one writing,
    // etc.
    let mut toggle = 0;
    #[allow(clippy::type_complexity)]
    let mut task =
        || -> Arc<dyn Fn(&str, &str) -> Result<(u16, String), ureq::Error> + Send + Sync> {
            toggle += 1;
            Arc::new(if toggle % 2 == 0 {
                |path, _| ureq_get_errors_are_ok(path)
            } else {
                ureq_post_errors_are_ok
            })
        };

    // Spawn the clients, some will read, some will write
    for i in 0..25 {
        let (out, path, task) = (taskout.clone(), addr.clone(), task());
        let body = format!("From thread {}", i);
        thread::spawn(move || out.send(task(&path, &body)).unwrap());
    }
    drop(taskout);

    // Assert client results
    let results = taskin.iter().collect::<Vec<_>>(); // debug
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
