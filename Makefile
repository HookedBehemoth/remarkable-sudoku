
TARGET ?= armv7-unknown-linux-gnueabihf

DEVICE_IP ?= '192.168.10.128'
DEVICE_HOST ?= root@$(DEVICE_IP)

all: build

.PHONY: build bench dist
build:
	cargo build --release --target=armv7-unknown-linux-gnueabihf

bench:
	cargo build --target=armv7-unknown-linux-gnueabihf --features "enable-runtime-benchmarking"

test:
	# Notice we aren't using the armv7 target here
	cargo test

deploy:
	ssh $(DEVICE_HOST) 'killall -q -9 sudoku || true; systemctl stop xochitl || true'
	scp ./target/$(TARGET)/release/sudoku $(DEVICE_HOST):/opt/bin/sudoku
	ssh $(DEVICE_HOST) 'RUST_BACKTRACE=1 RUST_LOG=debug /opt/bin/sudoku'

dist:
	tar -czvf src.tar.gz src Makefile Cargo.toml res sudoku.draft sudoku.oxide
	toltecmk

run: build deploy
