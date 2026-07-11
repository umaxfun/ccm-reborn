SHELL := /bin/sh

WINDOWS_TARGET ?= x86_64-pc-windows-msvc
LLVM_PREFIX := $(shell brew --prefix llvm 2>/dev/null)

ifneq ($(strip $(LLVM_PREFIX)),)
export PATH := $(LLVM_PREFIX)/bin:$(PATH)
endif

.DEFAULT_GOAL := help
.PHONY: help test mac mac-universal setup-win win all

help:
	@printf '%s\n' \
	  'make mac           Build a native macOS .app for this Mac.' \
	  'make mac-universal Build one .app for Apple Silicon and Intel Macs.' \
	  'make setup-win     Install the Windows cross-build prerequisites on macOS.' \
	  'make win           Build a 64-bit Windows NSIS setup .exe from macOS.' \
	  'make all           Build macOS and Windows artifacts.' \
	  'make test          Run frontend and Rust tests.'

test:
	npm run build
	cargo test --manifest-path src-tauri/Cargo.toml

mac:
	npm run tauri build -- --bundles app
	@printf '%s\n' 'macOS app: src-tauri/target/release/bundle/macos/CCM Reborn.app'

mac-universal:
	rustup target add aarch64-apple-darwin x86_64-apple-darwin
	npm run tauri build -- --target universal-apple-darwin --bundles app
	@printf '%s\n' 'Universal macOS app: src-tauri/target/universal-apple-darwin/release/bundle/macos/CCM Reborn.app'

setup-win:
	@command -v brew >/dev/null 2>&1 || { echo 'Homebrew is required for LLVM: https://brew.sh'; exit 1; }
	@command -v llvm-rc >/dev/null 2>&1 || { echo 'Installing LLVM (provides llvm-rc)…'; brew install llvm; }
	rustup target add $(WINDOWS_TARGET)
	cargo install --locked cargo-xwin
	@PATH="$$(brew --prefix llvm)/bin:$$PATH"; export PATH; command -v llvm-rc >/dev/null 2>&1 || { echo 'llvm-rc is not available after installing LLVM.'; exit 1; }

win: setup-win
	PATH="$$(brew --prefix llvm)/bin:$$PATH" npm run tauri build -- --runner cargo-xwin --target $(WINDOWS_TARGET) --bundles nsis
	@printf '%s\n' 'Windows installer: src-tauri/target/$(WINDOWS_TARGET)/release/bundle/nsis/'

all: mac win
