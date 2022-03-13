.PHONY: default build static clean
default: build

#
# This Makefile is mostly just in case I forget some of these Cargo commands.
# the commands are short enough that you might as well run cargo directly
#

SHELL = bash
out = httpfs
# out = ecurl

download:
	cargo fetch

build: download
	cargo build --release

# Adds some flags for static build
static: download
	export RUST_FLAGS="-C target-feature=+crt-static"
	cargo build --release

clean:
	cargo clean

test:
	cargo test