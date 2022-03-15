.PHONY: default build static clean
default: build

#
# This Makefile is mostly just in case I forget some of these Cargo commands.
# the commands are short enough that you might as well run cargo directly
#

SHELL = bash
out = httpfs
buildcmd = cargo build --workspace --release

download:
	cargo fetch

build: download
	$(buildcmd)

# Build statically linked against glibc
static: download
	RUSTFLAGS='-C target-feature=+crt-static' \
		$(buildcmd) \
			--target x86_64-unknown-linux-gnu

clean:
	cargo clean

test:
	cargo test