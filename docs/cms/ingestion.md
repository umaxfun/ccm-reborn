# Ingestion workflow

## Responsibilities

| Actor | Does | Does not do |
| --- | --- | --- |
| Code | Collects accessible messages, creates immutable Bronze data, produces conservative draft enrichment, validates JSON. | Choose semantic “latest” messages or publish content. |
| Agent | Reads the signed-in Discord UI where RPC is incomplete, maps sources, and proposes a source-backed draft. | Use a copied Discord user token, alter raw evidence, or publish content. |
| Human | Confirms facts, source selection, editorial tone, and publication. | Maintain a parallel file-based source map or editorial registry. |

## Collect Discord messages

Use local Discord RPC for accessible messages:

```sh
npm run discord:inventory
npm run discord:plan
npm run discord:export -- --from-sidebar --output work/discord-descriptions.json
npm run cms:bronze:import-rpc -- --input work/discord-descriptions.json
npm run cms:bronze:import-rpc -- --input work/discord-descriptions.json --apply
```

`discord:plan` produces only a local list of title-based candidates. It is not
a canonical relation and has no effect on Hygraph.

When the full post or an additional message is visible only in the signed-in
Discord UI, an agent creates an explicit Browser Bronze input and applies it:

```sh
npm run cms:bronze:import-browser -- --input work/browser-bronze.json
npm run cms:bronze:import-browser -- --input work/browser-bronze.json --apply
```

The input contains feeds and literal message captures. The importer validates
that each message URL matches its guild, channel, and message IDs, then computes
the fingerprint itself. Do not put summaries into `rawText`.

```json
{
  "format": 1,
  "feeds": [{
    "sourceKey": "discord:967712302767960064:123",
    "title": "Source thread title",
    "sourceUrl": "https://discord.com/channels/967712302767960064/123",
    "guildId": "967712302767960064",
    "channelId": "123",
    "kind": "DISCORD_FORUM_THREAD",
    "cadence": "WEEKLY"
  }],
  "observations": [{
    "sourceKey": "discord:967712302767960064:123",
    "messageId": "456",
    "messageUrl": "https://discord.com/channels/967712302767960064/123/456",
    "rawText": "Literal visible message text and links",
    "authorName": "Source author"
  }]
}
```

## Create a Silver candidate

For forum-card sources, create conservative dossiers and then enrich them from
the captured source:

```sh
npm run cms:silver:from-bronze
npm run cms:silver:from-bronze -- --apply
npm run cms:silver:enrich
npm run cms:silver:enrich -- --apply
```

These commands do not mutate Gold or publish anything. Their text and tags are
first-pass proposals. Use `--force` only when intentionally rebuilding a draft
that a reviewer has not protected with a decision.

## Select the evidence used by a dossier

An agent or reviewer explicitly selects the source messages that explain a
mod’s origin and latest relevant state:

```json
{
  "selections": [{
    "dossierKey": "example-mod",
    "originMessageId": "456",
    "latestUpdateMessageId": "789",
    "contextMessageIds": ["456", "789"],
    "latestKnownVersion": "1.2.0",
    "downloadUrl": "https://example.invalid/download",
    "sourceEvidence": "CONTENT_VERIFIED"
  }]
}
```

```sh
npm run cms:silver:select-sources -- --input work/source-selections.json
npm run cms:silver:select-sources -- --input work/source-selections.json --apply
```

The selected messages must already exist as Bronze observations of the
dossier’s primary source feed. The resulting `sourceContext` is stored in
Hygraph; the selection file is disposable after application.

## Submit editorial content for review

The agent creates a one-use batch containing a short description, at least two
body paragraphs, tags, choose/avoid guidance, and only verified version or
download details.

```sh
npm run cms:silver:agent-review -- --input work/editorial-batch.json
npm run cms:silver:agent-review -- --input work/editorial-batch.json --apply
```

The command sets the affected dossiers to `IN_REVIEW` and never touches Gold,
the production catalog, or R2. The next step is the human gate described in
[review-and-publication.md](review-and-publication.md).
