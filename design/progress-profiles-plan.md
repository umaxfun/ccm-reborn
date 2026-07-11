# Progress profiles and reversible campaign switching

This is the design target for the next implementation phase. The current
`Install` action is still a read-only dry-run; it does not yet apply profile
switches.

## 1. Stable identity and versioning

Catalog identity must distinguish a mod family from a major-version line:

- `familyId`: stable mod family, for example `nightmare`;
- `majorVersion`: `1`, `2`, ...;
- `campaignId`: the catalog row identity, for example `nightmare-v1`;
- `profileKey`: `${familyId}@${majorVersion}`.

Minor updates such as Nightmare 1.0 → 1.1 keep the same `profileKey` and
profile. They update the package and preserve the profile's bank, saves, and
progress. Nightmare 1.x and 2.x are separate catalog rows and separate
profiles. A major-version migration is explicit: v2 may be seeded from v1
only through a visible migration operation, while the v1 profile remains an
untouched rollback point.

The package SHA identifies an immutable package revision; it must not become
the profile identity. A manifest records the currently installed package SHA
and the migration history.

## 2. Profile manifest

Each managed profile lives under `~/.ccm-reborn/profiles/<profileKey>/` and has
an atomic `manifest.json`:

```json
{
  "schemaVersion": 1,
  "profileKey": "nightmare@1",
  "familyId": "nightmare",
  "majorVersion": 1,
  "campaignId": "nightmare-v1",
  "branch": "heart-of-the-swarm",
  "packageSha256": "...",
  "lastPlayedAt": 0,
  "lastPlayedSource": "app-launch",
  "files": [{"relativePath": "Banks/ZCampaign.SC2Bank", "sha256": "...", "size": 0, "kind": "bank"}],
  "progress": {
    "state": "inProgress",
    "lastMission": "ZLab2",
    "lastSuccessfulMission": "ZLab2",
    "lastMap": "ZStoryLab",
    "missionCompletedCount": 2
  }
}
```

`lastPlayedAt` is captured before copying. Prefer an app-launch timestamp;
fallback to the newest mtime among the owned bank and saves, and record the
source. Never use the mtime of a copied destination file.

## 3. Progress summaries and UI ordering

Add a read-only `list_campaign_progress` command. It returns one
`CampaignProgressSummary` per catalog row and one summary for discovered
managed profiles that are not in the catalog.

Each summary contains `profileKey`, branch, title/version, active/profile
source, state (`notStarted`, `inProgress`, `completed`, `unknown`),
`lastPlayedAt`, last mission/map, completed count, save count/bytes, bank
sections/keys/hash, the target `CampaignProgress.xml` node and warnings.

The bank is the source of mission details. `CampaignProgress.xml` only supplies
the target campaign's global flags and must be shown separately. Bank parsing
must use the campaign-last-info and campaign-stats sections, not duplicate
keys from story sections.

The dashboard groups by branch. Within each branch, sort only by:

1. profiles with progress before profiles without progress;
2. `lastPlayedAt` descending, null last;
3. lower-case title, then stable `campaignId`.

Thus Yuri and Abathur remain separate rows in HotS, while a 1.1 package update
does not create a second profile row for Nightmare 1.x.

## 4. Reversible switch operation

Every switch has an explicit source and target profile:

1. lock the operation and require StarCraft II to be fully closed;
2. classify only the active branch and exact source dependencies;
3. snapshot source files and manifest to the source `profileKey`;
4. restore the target profile if it exists, otherwise initialize a clean target
   bank/save set;
5. merge only the target node in `CampaignProgress.xml`, preserving every
   other branch/node byte-for-byte where possible;
6. stage campaign/dependency files, write a journal, and commit the new active
   profile atomically;
7. update the target manifest and remove the journal.

Unreadable or ambiguous MPQ saves are never owned by a profile and are never
removed. A major-version migration creates a new target profile and records
`migratedFrom`; it does not mutate the source profile.

## 5. Round-trip invariant

For `Yuri → Abathur → Yuri`, let `Y0` be the canonical sorted set of Yuri-owned
relative paths, bytes, and SHA-256 hashes, including the Yuri bank and target
progress node. After the two switches:

- Yuri's owned files and hashes equal `Y0` exactly;
- the complete `CampaignProgress.xml` equals its original bytes (non-target
  branches included);
- LotV, prologue/epilogue, and unrelated files are byte-for-byte unchanged;
- Abathur has its previous profile restored, or a clean profile if none existed;
- repeating a switch with unchanged hashes performs zero writes.

## 6. Required fixtures and tests

- parse fixture banks and extract last mission/map/completed count;
- deterministic per-branch progress sorting with null/tied timestamps;
- Yuri MPQ ownership versus Abathur ownership; unreadable MPQ stays untouched;
- vanilla first-install uses `vanilla-<branch>` and never claims custom saves;
- same-major package update preserves the profile and records the new SHA;
- major-version migration creates a separate profile and preserves the source;
- full Yuri → Abathur → Yuri integration test compares bytes and hashes and
  proves LotV/XML unrelated nodes are unchanged;
- interrupted journal recovery and manifest hash mismatch refuse partial
  restore;
- exported dry-run JSON is generated by the Rust planner, includes a schema
  version and complete source/destination operations, and never performs an
  install.
