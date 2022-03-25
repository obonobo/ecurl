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

build:
	$(buildcmd)
	cp target/release/httpfs ./

# Build statically linked against glibc
static:
	RUSTFLAGS='-C target-feature=+crt-static' \
		$(buildcmd) \
			--target x86_64-unknown-linux-gnu
	cp target/x86_64-unknown-linux-gnu/release/httpfs ./

clean:
	cargo clean
	rm -rf TEMP_*

test:
	cargo test
