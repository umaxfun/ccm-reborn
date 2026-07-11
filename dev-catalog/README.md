# Local catalog

`catalog.json` is the development source of truth. The desktop app starts with it selected when run through `npm run tauri dev`.

Each campaign uses a relative `package.path`; CCM Reborn resolves it from this directory. The ZIP files live in `dev-catalog/packages/`, so `catalog.json` and the package library form one self-contained local artifact. In production replace `path` with an HTTPS `url` while keeping the remaining package metadata unchanged.

CCM Reborn deliberately uses the existing CCM package convention — not a new archive format. Every ZIP must have exactly one `metadata.txt`; the `campaign=` field selects the original campaign directory, and everything next to that file is installed there. The manager stages the files, verifies the ZIP SHA-256, snapshots the whole target campaign directory, clears it, then copies the package in.

To add a package, put its ZIP in `dev-catalog/packages/` and add an entry to `catalog.json` with its byte size and SHA-256. The client refuses to install unverifiable archives. The ZIPs use the original CCM format: exactly one `metadata.txt`; UTF-8 and UTF-16 metadata are supported.
