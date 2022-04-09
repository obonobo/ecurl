# ecurl (ethan cURL) [![Build](https://github.com/obonobo/ecurl/actions/workflows/test.yml/badge.svg)](https://github.com/obonobo/ecurl/actions/workflows/test.yml)

_COMP-445_ <br />
_Data Communications & Computer Networks_ <br />
_Winter 2022 @ Concordia University_ <br />

This is a monorepo containing 3 assignments from the networking course:

1. `httpc`: an http client in Go, implements GET and POST using raw TCP.
2. `httpfs`: a multithreaded http file server in Rust, serves files from a
   specified directory, supports upload and download of files (GET + POST), uses
   raw TCP.
3. `udp`: a client + server reimplementation of the two above programs in Rust,
   using a custom protocol built on top of UDP. The protocol implements reliable
   transport using Selective Repeat ARQ (Automatic-Repeat-Request), and has a
   custom packet structure supporting different packet types such as ACK, SYN,
   DATA. The protocol is connection-oriented and requires an opening + closing
   handshake.
