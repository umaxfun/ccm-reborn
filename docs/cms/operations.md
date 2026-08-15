# CMS operations

## Environment

Keep secrets in the ignored `.env` file only.

| Variable | Used by |
| --- | --- |
| `HYGRAPH_TOKEN` | Hygraph Content and Management API authentication. |
| `HYGRAPH_CONTENT_ENDPOINT` | Bronze/Silver/Gold queries and mutations. |
| `HYGRAPH_HP_CONTENT_ENDPOINT` | Schema migrations and production catalog generation. |
| `HYGRAPH_MANAGEMENT_ENDPOINT` | Hygraph schema migrations. |
| `DISCORD_CLIENT_ID`, `DISCORD_CLIENT_SECRET`, `DISCORD_RPC_REDIRECT_URI` | Local Discord RPC OAuth. |
| `DISCORD_GUILD_ID` | Optional inventory restriction to one server. |
| `CLOUDFLARE_R2_PUBLIC_BASE_URL`, `CLOUDFLARE_R2_BUCKET` | R2 catalog upload. |

`work/discord-rpc-oauth.json` is a local OAuth token cache and must remain
untracked.

## Commands

| Goal | Dry run | Apply |
| --- | --- | --- |
| Create the base campaign schema | `npm run cms:schema` | append `-- --apply` |
| Create medallion models | `npm run cms:medallion:schema` | append `-- --apply` |
| Add selected source context | `npm run cms:medallion:source-context-schema` | append `-- --apply` |
| Add Silver difficulty | `npm run cms:medallion:difficulty-schema` | append `-- --apply` |
| Import Discord RPC evidence | `npm run cms:bronze:import-rpc -- --input <file>` | append `--apply` |
| Import Browser evidence | `npm run cms:bronze:import-browser -- --input <file>` | append `--apply` |
| Build forum candidates | `npm run cms:silver:from-bronze` | append `--apply` |
| Enrich candidate drafts | `npm run cms:silver:enrich` | append `--apply` |
| Apply an editorial review batch | `npm run cms:silver:agent-review -- --input <file>` | append `--apply` |
| Select source messages | `npm run cms:silver:select-sources -- --input <file>` | append `--apply` |
| Generate public JSON | `npm run cms:generate` | append `--upload` after review |

Use `--apply` only after reading the command’s plan and checking the explicit
input file. `--upload` is never implied by `--apply`.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `HYGRAPH_* is required` | Load `.env` and use the endpoint required by that command. |
| Hygraph `401` or `403` | Validate the token and Content/Management permissions. |
| Discord RPC has no IPC socket | Start and sign in to Discord Desktop, then repeat OAuth. |
| Browser Bronze rejects a URL | Correct the guild/channel/message IDs in the literal capture; do not weaken the validator. |
| Source selection cannot find a message | Import it into Bronze first and verify that it belongs to the dossier’s primary feed. |
| Catalog generation reports a removal | Treat it as a safety check, not an error to bypass. |

## Verification

After changing CMS scripts or catalog mapping:

```sh
node --check scripts/hygraph/<changed-script>.mjs
npm run cms:verify-local
npm run build
```

For every applied content operation, record the number of feeds,
observations, dossiers, and Gold records affected, plus whether anything was
published. Do not include tokens or OAuth credentials in that record.
