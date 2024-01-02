
TARGET ?= armv7-unknown-linux-gnueabihf

DEVICE_IP ?= '192.168.10.128'
DEVICE_HOST ?= root@$(DEVICE_IP)

all: build

.PHONY: build
build:
	cargo build --release --target=armv7-unknown-linux-gnueabihf

bench:
	cargo build --target=armv7-unknown-linux-gnueabihf --features "enable-runtime-benchmarking"

test:
	# Notice we aren't using the armv7 target here
	cargo test

deploy:
	ssh $(DEVICE_HOST) 'killall -q -9 rm-sudoku || true; systemctl stop xochitl || true'
	scp ./target/$(TARGET)/release/rm-sudoku $(DEVICE_HOST):/opt/bin/sudoku
	ssh $(DEVICE_HOST) 'RUST_BACKTRACE=1 RUST_LOG=debug /opt/bin/sudoku'

dist: build
	mkdir -p ./dist/opt/bin
	mkdir -p ./dist/opt/usr/share/applications
	cp ./target/$(TARGET)/release/rm-sudoku ./dist/opt/bin/sudoku
	cp ./sudoku.oxide ./dist/opt/usr/share/applications/
	tar -czvf dist/sudoku.tar.gz dist/opt/*

run: build deploy
