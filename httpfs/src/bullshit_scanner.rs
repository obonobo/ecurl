///
/// This is a port of buffered_scanner.go
///
use self::constants::*;
use core::slice;
use std::{io::Read, rc::Rc};

use self::errors::{BullshitError, Result};

/// scanner-related constants
pub mod constants {
    pub const MIN_BUFSIZE: usize = 1 << 6; // 64B
    pub const MAX_BUFSIZE: usize = 1 << 27; // 128MB
    pub const DEFAULT_BUFSIZE: usize = 1 << 20; // 1MB
}

pub struct Buffer {
    red: usize,
    filled: usize,
    bites: Vec<u8>,
}

/// Renamed to [BullshitScanner] because this is getting so much more
/// complicated than Go
pub struct BullshitScanner<'a> {
    reader: &'a mut dyn Read,

    /// The scanner's internal buffer
    buf: Buffer,

    /// An error registered on the scanner
    err: Option<Rc<BullshitError>>,
}

impl<'a> BullshitScanner<'a> {
    /// Creates a new BufferedScanner with the default buffer capacity
    pub fn new(reader: &'a mut dyn Read) -> Self {
        Self::with_capacity(reader, DEFAULT_BUFSIZE)
    }

    /// Creates a new BufferedScanner
    pub fn with_capacity(reader: &'a mut dyn Read, size: usize) -> Self {
        use std::cmp::{max, min};
        let capacity = min(max(size, MIN_BUFSIZE), MAX_BUFSIZE);
        Self {
            reader,
            err: None,
            buf: Buffer {
                red: 0,
                filled: 0,
                bites: vec![0; capacity],
            },
        }
    }

    pub fn next_line(&mut self) -> Result<(String, usize)> {
        if self.cannot_read_anymore() {
            return Err(self.err.clone().unwrap());
        }
        self.load_empty();

        let buf = &self.buf.bites[self.buf.red..];
        match Self::scan_line(buf) {
            Ok((line, n)) => {
                self.buf.red += n;
                Ok((line, n))
            }
            Err(e) => {
                if let Some(e) = self.err.clone() {
                    // We have reached EOF and there are no lines left in the
                    // buffer
                    return Err(e);
                }

                if self.buf.red == 0 {
                    // Then our buffer is not big enough to handle the line
                    return Err(Rc::new(
                        BullshitError::new()
                            .wrap(Box::new(e))
                            .msg("buffer is not big enough"),
                    ));
                }

                // Otherwise, we may need to load more data. Discard the read
                // portion of the buffer and read from the socket again
                self.load();

                // After loading, we can retry this operation. Recursion is
                // broken by the branches at the top of this scope - this
                // function should not recurse more than once
                self.next_line()
            }
        }
    }

    pub fn next_byte(&mut self) -> Result<u8> {
        if self.cannot_read_anymore() {
            let err = self.err.clone();
            let err = err.unwrap(); // Won't panic
            return Err(err);
        }
        self.load_empty();

        if self.buf.red < self.buf.filled {
            let b = self.buf.bites[self.buf.red];
            self.buf.red += 1;
            return Ok(b);
        }

        // Otherwise, we need to load more data
        self.load();
        self.next_byte()
    }

    fn cannot_read_anymore(&self) -> bool {
        self.err.is_some() && self.buf.red == self.buf.filled
    }

    /// Discards the unread portion of the buffer and loads more data from the
    /// reader
    fn load(&mut self) {
        use std::cmp::min;

        let cap = self.buf.bites.len();
        let (left, right) = self.buf.bites[..cap].split_at_mut(self.buf.red);
        let n = min(left.len(), right.len());
        left[..n].copy_from_slice(&mut right[..n]);
        self.buf.filled = self.buf.filled - self.buf.red;
        self.buf.red = 0;

        match self.reader.read(&mut self.buf.bites[self.buf.filled..]) {
            // Reslice
            Ok(n) => {
                self.buf.filled += n;
                if n == 0 {
                    self.err = Some(Rc::new(BullshitError::new().msg("EOF")));
                }
            }

            // Register the error on the scanner
            Err(e) => self.err = Some(Rc::new(BullshitError::new().wrap(Box::new(e)))),
        }
    }

    fn load_empty(&mut self) {
        if self.buf.filled == 0 && !self.cannot_read_anymore() {
            self.load();
        }
    }

    fn scan_line(buf: &[u8]) -> core::result::Result<(String, usize), BullshitError> {
        use std::str::from_utf8;
        for (i, b) in buf.iter().enumerate() {
            if *b == b'\n' {
                let map_err =
                    from_utf8(&buf[..i]).map_err(|e| BullshitError::wrapping(Box::new(e)))?;
                return Ok((String::from(map_err.trim_end_matches(['\r', '\n'])), i + 1));
            }
        }

        Err(BullshitError::new().msg(&format!("read {} bytes without a newline", buf.len())))
    }

    /// Note that the iterator will stop once there are no more newline
    /// delimited tokens in the string - there may still be some bytes left, the
    /// [BullshitScanner] is meant to provide fine-grained control over reading.
    pub fn lines(&'a mut self) -> iterators::Lines<'a> {
        iterators::Lines { inner: self }
    }

    /// Works similar to [Read::bytes] except it doesn't completely transform
    /// the reader, it only borrows and only consumes as many bytes as you take
    /// from the [iterator](Iterator)
    ///
    /// I named this function [`bites`](`BullshitScanner::bites`) just so that
    /// it is a bit easier to call without confusing [Read::bytes]
    pub fn bites(&'a mut self) -> iterators::Bytes<&'a mut BullshitScanner> {
        iterators::Bytes { inner: self }
    }
}

mod iterators {
    use super::*;

    pub struct Lines<'a> {
        pub inner: &'a mut BullshitScanner<'a>,
    }

    impl<'a> Iterator for Lines<'a> {
        type Item = (String, usize);

        fn next(&mut self) -> Option<Self::Item> {
            self.inner.next_line().map(Some).ok()?
        }
    }

    pub struct Bytes<R: Read> {
        pub inner: R,
    }

    impl<'a> Iterator for Bytes<&'a mut BullshitScanner<'a>> {
        type Item = core::result::Result<u8, std::io::Error>;

        fn next(&mut self) -> Option<Self::Item> {
            let mut b: u8 = 0;
            loop {
                return match self.inner.read(slice::from_mut(&mut b)) {
                    Ok(0) => None,
                    Ok(..) => Some(Ok(b)),
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => Some(Err(e)),
                };
            }
        }
    }
}

impl<'a> Read for BullshitScanner<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.load_empty();
        let mut red = 0;

        if self.cannot_read_anymore() {
            return Ok(0);
        }

        while red < buf.len() {
            if self.cannot_read_anymore() {
                return Ok(red);
            }

            // Copy as many bytes as possible from the internal buffer
            let src = &self.buf.bites[self.buf.red..self.buf.filled];
            let n = std::cmp::min(src.len(), buf.len());
            let src = &src[..n];
            let buff = &mut buf[red..red + n];
            buff.copy_from_slice(src);
            self.buf.red += n;
            red += n;

            // If we have satisfied the read request, then we are done
            if red == buf.len() {
                break;
            }

            // Otherwise, we need to load more data
            self.load();
        }

        Ok(red)
    }
}

pub mod errors {
    use std::rc::Rc;

    pub static EOF: &str = "EOF";

    pub type Result<T> = core::result::Result<T, Rc<BullshitError>>;
    pub type WrappableError = Box<dyn std::error::Error>;

    /// An simple error returned by the [`super::BufferedScanner`]. [Error] may
    /// wrap [`std::io::Error`], and it may include a message.
    #[derive(Debug)]
    pub struct BullshitError {
        msg: Option<String>,
        err: Option<WrappableError>,
    }

    impl BullshitError {
        pub fn new() -> Self {
            Self {
                msg: None,
                err: None,
            }
        }

        pub fn with_msg(msg: String) -> Self {
            Self::of(Some(msg), None)
        }

        pub fn wrapping(err: Box<dyn std::error::Error>) -> Self {
            Self::of(None, Some(err))
        }

        pub fn of(msg: Option<String>, err: Option<Box<dyn std::error::Error>>) -> Self {
            Self { msg, err }
        }

        /// Returns an EOF error
        pub fn default() -> Self {
            Self::new().msg(EOF)
        }

        /// Transforms the message of this error
        pub fn msg(self, msg: &str) -> Self {
            Self {
                msg: Some(String::from(msg)),
                ..self
            }
        }

        pub fn wrap(self, err: Box<dyn std::error::Error>) -> Self {
            Self {
                err: Some(err),
                ..self
            }
        }
    }

    impl std::error::Error for BullshitError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(self.err.as_ref()?.as_ref())
        }
    }

    impl std::fmt::Display for BullshitError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.msg.clone().unwrap_or(String::from("")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, str::from_utf8};

    const BUFSIZES: [usize; 8] = [1, 1 << 1, 1 << 2, 1 << 4, 1 << 6, 1 << 10, 1 << 20, 1 << 27];

    /// Test case macro for running the scanner on different inputs, with
    /// different buffer sizes
    macro_rules! read_with_different_bufsize_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (input, bufsizes) = $value;
                for bufsize in bufsizes.iter() {
                    let mut reader = stringreader::StringReader::new(&input);
                    let mut scnr = BullshitScanner::with_capacity(&mut reader, *bufsize);
                    let mut red = String::new();
                    let res = scnr.read_to_string(&mut red).unwrap();

                    assert_eq!(res, input.len());
                    assert_eq!(red, input);
                }
            }
        )*
        };
    }

    read_with_different_bufsize_tests! {
        empty: ("", BUFSIZES),

        hello_world: ("Hello world!", BUFSIZES),

        lorem_ipsum: ("
        Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
        incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
        nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
        Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
        fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
        culpa qui officia deserunt mollit anim id est laborum.
        ", BUFSIZES),

        big: ((0..1000).map(|_| "big!\n").collect::<String>(), BUFSIZES),

        very_big: (std::iter::repeat("very big!\n").take(1<<16).collect::<String>(), BUFSIZES),
    }

    macro_rules! bites_iterator_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (input, expected, take) = $value;
                assert_bites_iterator(input, expected, take);
            }
        )*
        };
    }

    bites_iterator_tests! {
        hello_world_asdasdasdasd: ("Hello world!asdasdasdasd", "Hello", 5),
    }

    fn assert_bites_iterator(input: &str, expected: &str, take: usize) {
        let mut reader = stringreader::StringReader::new(input);
        let mut scnr = BullshitScanner::new(&mut reader);

        let partial = BullshitScanner::bites(&mut scnr)
            .map(|res| res.unwrap())
            .take(take)
            .collect::<Vec<_>>();

        let out = from_utf8(&partial).unwrap();
        assert_eq!(expected, out);
    }

    #[test]
    fn test_lines_iterator() {
        let data = "
        Hello world!
        asd
        asd
        as
        da
        sd
        ";
        let mut reader = stringreader::StringReader::new(data);
        let mut scnr = BullshitScanner::new(&mut reader);
        let out = scnr.lines().map(|l| l.0).collect::<String>();
        assert_eq!(data.replace("\n", "").trim_end(), out);
    }
}
