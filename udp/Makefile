.PHONY: default build static clean copy
default: build

#
# This Makefile is mostly just in case I forget some of these Cargo commands.
# the commands are short enough that you might as well run cargo directly
#

SHELL = bash
buildcmd = cargo build --workspace --release
copycmd = cp ./target/*/release/{server,client} ./

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

# Runs the router script with some default arguments
router:
	./resources/a3/router.go --drop-rate 0 --max-delay 0

copy:
	$(copycmd)
