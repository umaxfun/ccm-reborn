SHELL := /bin/sh

.DEFAULT_GOAL := help
.PHONY: help check source-size test cli mac mac-universal setup-win win linux all

help:
	@printf '%s\n' \
	  'make mac           Build native macOS .app and .dmg artifacts.' \
	  'make mac-universal Build one .app for Apple Silicon and Intel Macs.' \
	  'make setup-win     Install the Windows cross-build prerequisites on macOS.' \
	  'make win           Build a 64-bit Windows NSIS setup .exe from macOS.' \
	  'make linux         Build Linux .deb and AppImage artifacts through Docker.' \
	  'make all           Build macOS, Windows, and Linux artifacts.' \
	  'make test          Run frontend and Rust tests.' \
	  'make check         Run source-size checks, frontend and Rust tests.' \
	  'make cli           Build and show the core CLI help.'

source-size:
	npm run check:source-size

check: source-size test

test:
	npm run build
	cargo test --manifest-path src-tauri/Cargo.toml

cli:
	cargo run --manifest-path src-tauri/Cargo.toml --bin ccm -- help

mac:
	npm run tauri:mac

mac-universal:
	rustup target add aarch64-apple-darwin x86_64-apple-darwin
	npm run tauri build -- --target universal-apple-darwin --bundles app
	@printf '%s\n' 'Universal macOS app: src-tauri/target/universal-apple-darwin/release/bundle/macos/CCM Reborn.app'

setup-win:
	node scripts/release.mjs setup-win

win:
	npm run tauri:win

linux:
	npm run tauri:linux

all:
	npm run tauri:all
