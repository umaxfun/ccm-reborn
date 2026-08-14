# CCM Reborn

> Play StarCraft II community campaigns without manually replacing maps and
> losing track of what you installed or where you left off.

[Download the latest release](https://github.com/umaxfun/ccm-reborn/releases/latest)
· [Report a problem](https://github.com/umaxfun/ccm-reborn/issues)

CCM Reborn is a desktop manager for the community campaigns made for the
original StarCraft II Custom Campaign Manager (CCM). Pick a campaign from a
catalogue, review the change, and play it — the app keeps each vanilla campaign
branch and its progress separate.

## Get playing

1. Download the build for your system from the
   [latest release](https://github.com/umaxfun/ccm-reborn/releases/latest):
   universal `.dmg` for macOS, `setup.exe` for Windows, or `.deb` / AppImage
   for Linux. On Linux, use the `.deb` on Debian/Ubuntu and the AppImage on
   most other desktop distributions.
2. Open **Sources** and choose your StarCraft II folder — the folder containing
   `Maps` (and usually `SC2Data`). Then select a community cloud catalogue or
   a local `catalog.json`.
3. Open **Campaigns**, find a campaign in its StarCraft II branch, and choose
   **Install**. CCM Reborn shows a review first; select **Apply installation**
   only after you are happy with the files and save/profile changes.
4. Press **Open Battle.net**, then press **Play** in Battle.net to launch
   StarCraft II. When you return, the campaign card tells you which campaign is
   installed and, when a compatible save is found, where to continue.

## What CCM Reborn does for you

- Keeps Wings of Liberty, Heart of the Swarm, Legacy of the Void, and Nova
  Covert Ops as separate campaign branches, so changing one does not replace
  the others.
- Sorts alternatives you have played by recent progress; campaigns with no
  detected progress stay alphabetised below them.
- Matches saves and campaign progress to the selected StarCraft II account
  profile, rather than treating a save from another account as yours.
- Lets you switch between a community cloud catalogue and a local catalogue —
  useful both for regular play and trying a package before it is published.

## Your campaigns and saves stay in your control

Close StarCraft II before installing, repairing, or restoring a campaign. An
install is reviewed before it changes anything: CCM Reborn verifies the
download checksum, stages the package, takes a snapshot of the selected
campaign branch, and records exactly which files it manages. If a step fails,
it rolls the change back. **Restore original** restores only that selected
branch; it does not reset the other campaigns.

The app may ask you to choose an SC2 account profile. This is intentional: it
keeps campaign progress associated with the account that earned it. Older CCM
snapshots are left alone until you explicitly migrate or remove them.

## Campaign catalogues

A catalogue is simply a list of campaigns and verified download links. CCM
Reborn can use a community cloud catalogue or a local `catalog.json`; it checks
each package's SHA-256 and size before installing it. A catalogue source is
always selected explicitly in **Sources**.

## Need help?

Please [open an issue](https://github.com/umaxfun/ccm-reborn/issues) with your
operating system, CCM Reborn version, the campaign you selected, and the error
text or screenshot. Do not attach personal StarCraft II account files or saves
unless someone has specifically explained why they are needed.

---

## Development and catalogue maintenance

The sections below are for contributors, catalogue publishers, and anyone
building CCM Reborn locally. Players normally only need the release above.

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

### Run locally

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

## Discord description export

The Discord exporter is a local, read-only integration with the Discord Desktop
client. It uses Discord's documented local RPC OAuth flow (`rpc`,
`messages.read`, and `guilds`); it is not a bot, does not join a server, and
does not use a copied Discord user token. Keep the Desktop client running and
logged into the account that can view the source threads.

Create a Discord Application in the Developer Portal. In its OAuth settings,
register `http://localhost:3344/discord-rpc` as a redirect URI (or use another
URI consistently in both places), then add the application credentials to the
ignored `.env` file:

```sh
DISCORD_CLIENT_ID=...
DISCORD_CLIENT_SECRET=...
DISCORD_RPC_REDIRECT_URI=http://localhost:3344/discord-rpc
DISCORD_GUILD_ID=967712302767960064
DISCORD_RPC_FORUM_CHANNEL_ID=1125272260249395340
```

The first run opens Discord's own authorization prompt. Start by producing a
small, no-message inventory for the configured server:

```sh
npm run discord:inventory
```

It writes `work/discord-rpc-inventory.json`, including any thread channels the
client exposes. `DISCORD_RPC_FORUM_CHANNEL_ID` filters those candidates to the
specified forum; it does not grant any additional access. Review it, then either add the selected IDs to
`DISCORD_RPC_THREAD_IDS` as a comma-separated list or export all threads found
in that reviewed inventory:

```sh
npm run discord:export
npm run discord:export -- --from-inventory
```

The exporter writes `work/discord-descriptions.json`. It keeps source IDs,
timestamps, author display names, attachment links, Discord URLs, and raw
reaction counts when Discord RPC provides them. Each message and thread has a
`reactionDataAvailable` flag, so a missing RPC reaction field is never mistaken
for zero reactions. The later Hygraph import can therefore be reviewed and
traced to the original thread. It never writes to Discord or Hygraph.

After the first Discord approval, the exporter caches its short-lived OAuth
access and refresh tokens in the ignored `work/discord-rpc-oauth.json` file
(owner-only permissions on macOS). It reuses or refreshes that authorization
without another consent prompt; the cache contains no client secret and is
never committed.

For the normal text channels that have one mod per channel, generate a proposed
catalog mapping before exporting anything. The matcher only writes candidates;
it never treats a name match as permission to read messages:

```sh
npm run discord:plan
```

Review `work/discord-campaign-channel-plan.json` and select campaign/channel
pairs explicitly. A normal channel can then be fetched by its ID with
`npm run discord:export -- --channel <channel-id>`. The legacy `--thread` form
remains available for individual forum posts.

The reviewed source map lives in
`scripts/discord/campaign-source-map.json`. It maps every current production
campaign to a Discord channel or forum thread, with evidence marked as
`content-verified`, `name-match`, or `shared-reference`. Check that it still
covers the production catalog after any edit:

```sh
npm run discord:map:check
```

Once that source map has been reviewed, export exactly its sources (deduplicating
shared channels while retaining the campaign-to-source mapping) with:

```sh
npm run discord:export-mapped
```

The reviewed English card copy and controlled classification live in
`scripts/hygraph/campaign-editorial.json`. It is deliberately separate from the
package catalog: it can evolve without changing a package URL, version, or
checksum. Validate that it covers every current campaign, uses only controlled
tags, and still has a reviewed Discord source for each entry:

```sh
npm run cms:editorial:check
```

Preview the CMS mutation without writing, then create Hygraph drafts only when
the copy is approved. Publishing is an explicit third step:

```sh
npm run cms:editorial:import
npm run cms:editorial:import -- --apply
npm run cms:editorial:import -- --apply --publish
```

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
