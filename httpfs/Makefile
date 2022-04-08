.PHONY: default build static clean copy
default: build

#
# This Makefile is mostly just in case I forget some of these Cargo commands.
# the commands are short enough that you might as well run cargo directly
#

SHELL = bash
buildcmd = cargo build --workspace --release
copycmd = cp ./target/*/release/httpfs ./

download:
	cargo fetch

build:
	$(buildcmd)

# Build statically linked against glibc
static:
	RUSTFLAGS='-C target-feature=+crt-static' $(buildcmd) --target x86_64-unknown-linux-gnu

clean:
	cargo clean
	rm -rf TEMP_*

test:
	cargo test

copy:
	$(copycmd)