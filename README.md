# CCM Reborn

Cross-platform desktop manager for StarCraft II Custom Campaign Manager packages. It is built with Tauri 2, Rust, and a small TypeScript UI.

CCM Reborn installs the existing CCM ZIP layout; it does **not** invent a package format. A valid package has one `metadata.txt`; its `campaign=` value determines the campaign destination:

| `metadata.txt` value | SC2 directory managed by CCM Reborn |
| --- | --- |
| Wings / Liberty / WoL | `Maps/Campaign` |
| Heart / Swarm / HotS | `Maps/Campaign/swarm` |
| Legacy / Void / LotV | `Maps/Campaign/void` |
| Nova / Covert / NCO | `Maps/Campaign/nova` |

Before installing, the app checks the ZIP SHA-256, stages its contents, snapshots its managed campaign branch, clears it, and then copies the package. WoL deliberately leaves sibling campaign branches such as `swarm` and `void` alone. **Restore original campaigns** targets one declared campaign slot and restores that slot’s snapshot. A small journal handles a crash in the middle of a copy.

Successful installs also write an exact package inventory to
`<game>/.ccm-reborn/installed/`. It contains every archive member copied to the
game, its destination, size, and SHA-256. When a package is updated, the new
archive is verified and staged first; then the previous managed files are
verified and restored or removed before the new files are copied. A changed or
missing old file stops the update rather than deleting an unowned path. The
previous inventory is retained under `installed/history/` before it is replaced.

## Local development

The local artifact is `dev-catalog/catalog.json` together with the 37 verified CCM ZIPs in `dev-catalog/packages/`. Every package path is relative to the catalog, so this folder can be copied or published as one self-contained catalog bundle.

```sh
npm install
npm run tauri dev
```

The `tauri dev` script selects a free localhost port each time, so it does not collide with other Vite/Tauri projects.

**Install** first produces a read-only dry-run: it validates the archive,
enumerates campaign/dependency operations and the bank/save/progress changes
CCM can verify (ambiguous or unreadable saves are called out rather than
guessed). Once an explicit StarCraft II account profile is selected in
**Sources**, the review dialog enables **Apply installation**. The same
transactional Rust core is used by the UI and `ccm install`; it requires SC2
to be closed and rolls back profile and campaign files on failure.

The broader profile, progress ordering, version identity, migration, and
Yuri → Abathur → Yuri round-trip design is documented in
[`design/progress-profiles-plan.md`](design/progress-profiles-plan.md).

The standalone CLI uses the same Rust core as the Tauri UI and is available without launching the GUI:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --bin ccm -- help
cargo run --manifest-path src-tauri/Cargo.toml --bin ccm -- plan \
  --game-dir /path/to/fixture-game --archive /path/to/package.zip \
  --campaign-id nightmare-v1 --title "Nightmare" --sha256 HEX \
  --output dry-run.json
cargo run --manifest-path src-tauri/Cargo.toml --bin ccm -- installed \
  --game-dir "/path/to/StarCraft II" --target Maps/Campaign/swarm
cargo run --manifest-path src-tauri/Cargo.toml --bin ccm -- install \
  --game-dir "/path/to/StarCraft II" --archive /path/to/package.zip \
  --campaign-id nightmare-v1 --title "Nightmare" --sha256 HEX \
  --profile-dir "/path/to/StarCraft II/Accounts/..." --confirm APPLY
cargo run --manifest-path src-tauri/Cargo.toml --bin ccm -- restore \
  --game-dir "/path/to/StarCraft II" --profile-dir "/path/to/StarCraft II/Accounts/..." \
  --target Maps/Campaign/swarm --confirm RESTORE
cargo run --manifest-path src-tauri/Cargo.toml --bin ccm -- summary \
  --root /path/to/fixture-profile --output fixture-summary.json
cargo run --manifest-path src-tauri/Cargo.toml --bin ccm -- profile-key \
  --family nightmare --major 1 --campaign-id nightmare-v1
cargo run --manifest-path src-tauri/Cargo.toml --bin ccm -- sort-summary \
  --input progress-summary.json --output sorted-summary.json
cargo run --manifest-path src-tauri/Cargo.toml --bin ccm -- roundtrip-check \
  --root /path/to/restored/profile --manifest /path/to/yuri/manifest.json
```

All commands are read-only except `install` and `restore`; each requires its
literal confirmation token after reviewing the relevant plan/state. For a
live game, `install` and `restore` also require the exact `--profile-dir`.
`restore` always requires one explicit campaign `--target`; it never guesses
which of the four campaign slots you meant.

Choose the directory that contains SC2's `Maps` directory (on Windows, it normally also contains `SC2Data` and `Support64`) in **Configure sources**. Use a disposable SC2 install for development.

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
        "campaign": "Heart of the Swarm"
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

[`catalog/catalog.json`](catalog/catalog.json) is the production source of truth.
It contains only public HTTPS URLs, SHA-256 checksums, and byte sizes; the ZIP
files do not need to stay in the repository after publication.

To add or update a campaign, give its local ZIP a short lowercase slug. The
publisher reads its `metadata.txt`, validates the archive, uploads an immutable
object to R2, writes a history snapshot, and publishes `catalog.json` last:

```sh
npm run catalog:publish -- ~/Downloads/LOTVRogue.zip --slug artanis-rogue
```

Pass `--id`, `--version`, `--title`, `--author`, `--description`, or
`--campaign wol|hots|lotv|nco` only when the ZIP metadata needs correction.
Use `--dry-run` to validate and show the intended URL without changing R2 or
the catalog. Existing R2 objects are never overwritten; publish a corrected
archive with a new version or slug.

The publisher creates readable immutable archive names such as
`campaigns/hots/yuri-1.09.zip` and replaces local `path` with HTTPS `url`:

```json
"package": {
  "url": "https://catalog.example.com/campaigns/hots-randomizer/1.0.2.zip",
  "sha256": "<sha256>",
  "size": 31947366
}
```

Point **Configure sources** at the HTTPS URL for `catalog.json`. The Rust backend fetches the catalog and package, so browser CORS is not part of the client path.

After publishing, verify every public package size with `npm run catalog:smoke`.
Add `-- --verify f-yuri-of-the-swarm` to download and verify one package's SHA-256 too.

## Checks

```sh
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --debug --bundles app
```

## Release builds

`npm run tauri build` remains the native Tauri command: it builds for the current
platform and all of its configured bundle formats. Release shortcuts are also
available:

```sh
npm run tauri:mac    # macOS .app and .dmg (macOS host)
npm run tauri:mac:universal # universal macOS .app and .dmg (macOS host)
npm run tauri:win    # x64 Windows NSIS setup.exe (Windows or macOS host)
npm run tauri:linux  # x64 .deb and AppImage (Linux or macOS with Docker)
npm run tauri:all    # all of the above from macOS
```

On macOS, `tauri:win` installs the Windows Rust target, `cargo-xwin`, Homebrew
LLVM, and NSIS on first use. Linux artifacts are built in an `linux/amd64`
Docker image and copied into `src-tauri/target/release/bundle/`. `tauri:all`
must run on macOS because macOS bundles require a Mac build host.

The existing Makefile mirrors these commands; `make mac-universal` still builds
one `.app` for Apple Silicon and Intel Macs.

## GitHub Actions

CI runs the frontend build and Rust tests on macOS, Windows, and Linux for every
push and pull request. The **Release** workflow builds a universal macOS DMG,
an x64 Windows NSIS installer, and x64 Linux `.deb` and AppImage packages, then
attaches them to a GitHub Release.

Trigger it by pushing a matching version tag after bumping the manifests:

```sh
npm run version:bump
git commit -am "Release v0.1.1"
git tag v0.1.1
git push --follow-tags
```

It can also be started manually from the Actions tab; the requested version must
match the manifests.

## Version bump

Use the version script before a release to update the application version in the
Node, Rust, lockfile, and Tauri manifests together:

```sh
npm run version:bump          # 0.1.0 → 0.1.1 (patch by default)
npm run version:bump -- patch  # 0.1.0 → 0.1.1
npm run version:bump -- minor  # 0.1.0 → 0.2.0
npm run version:bump -- 1.0.0  # set an exact semver version
```

Add `--dry-run` to preview the result. The script refuses to write when those
manifest versions are already out of sync. Run `node scripts/bump-version.mjs
--check` to validate their synchronization without changing the version.
