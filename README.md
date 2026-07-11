# CCM Reborn

Cross-platform desktop manager for StarCraft II Custom Campaign Manager packages. It is built with Tauri 2, Rust, and a small TypeScript UI.

CCM Reborn installs the existing CCM ZIP layout; it does **not** invent a package format. A valid package has one `metadata.txt`; its `campaign=` value determines the campaign destination:

| `metadata.txt` value | SC2 directory managed by CCM Reborn |
| --- | --- |
| Wings / Liberty / WoL | `Maps/Campaign` |
| Heart / Swarm / HotS | `Maps/Campaign/swarm` |
| Legacy / Void / LotV | `Maps/Campaign/void` |
| Nova / Covert / NCO | `Maps/Campaign/nova` |

Before installing, the app checks the ZIP SHA-256, stages its contents, snapshots the entire target campaign directory, clears it, and then copies the package. **Restore original campaigns** clears that target again and restores the snapshot. A small journal handles a crash in the middle of a copy.

## Local development

The local artifact is `dev-catalog/catalog.json` together with the 37 verified CCM ZIPs in `dev-catalog/packages/`. Every package path is relative to the catalog, so this folder can be copied or published as one self-contained catalog bundle.

```sh
npm install
npm run tauri dev
```

The `tauri dev` script selects a free localhost port each time, so it does not collide with other Vite/Tauri projects.

Choose the directory that contains SC2's `Maps` directory (on Windows, normally the directory containing `SC2_x64.exe`) in **Configure sources**. Use a disposable SC2 install for development.

Each catalog entry requires metadata for display and a verified package:

```json
{
  "format": 1,
  "name": "My local catalog",
  "updatedAt": "2026-07-09T00:00:00Z",
  "campaigns": [
    {
      "id": "hots-randomizer",
      "title": "Heart of the Swarm Randomizer",
      "author": "Kit",
      "version": "1.0.2",
      "description": "A short description.",
      "tags": ["HotS", "randomizer"],
      "requirements": {
        "campaign": "Heart of the Swarm",
        "platforms": ["Windows", "macOS", "Linux / Wine"]
      },
      "package": {
        "path": "./packages/hots-randomizer.zip",
        "sha256": "<64 lowercase hexadecimal characters>",
        "size": 31947366
      }
    }
  ]
}
```

`package.path` is for a catalog selected from disk and is resolved relative to `catalog.json`. The catalog source itself is selected deliberately by the user in the app.

## Cloudflare publishing

Put immutable ZIPs in R2, for example `campaigns/hots-randomizer/1.0.2.zip`, then publish the same catalog JSON from a Worker or public bucket. Replace local `path` with HTTPS `url`:

```json
"package": {
  "url": "https://catalog.example.com/campaigns/hots-randomizer/1.0.2.zip",
  "sha256": "<sha256>",
  "size": 31947366
}
```

Point **Configure sources** at the HTTPS URL for `catalog.json`. The Rust backend fetches the catalog and package, so browser CORS is not part of the client path.

## Checks

```sh
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --debug --bundles app
```

## Release builds

From macOS, use the included Makefile:

```sh
make mac            # native .app for the current Mac
make mac-universal  # one .app for Apple Silicon and Intel Macs
make win            # x64 Windows NSIS setup.exe, cross-compiled on macOS
```

`make win` installs the Windows Rust target, `cargo-xwin`, and Homebrew LLVM on first use. It produces an NSIS installer in `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/`. A Windows `.msi` must still be built on Windows; the macOS target intentionally emits the portable NSIS setup `.exe` instead.
