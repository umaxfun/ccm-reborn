# CMS content pipeline

Hygraph is the working source of truth for CCM Reborn mod content. It stores
Discord evidence, editorial drafts, review status, and published campaign
records. The JSON file in R2 is a generated delivery artifact for the app.

## Documentation

| Document | Scope |
| --- | --- |
| [architecture.md](architecture.md) | Bronze, Silver, and Gold models; identity and data ownership. |
| [ingestion.md](ingestion.md) | Discord collection and the path from raw messages to a reviewable draft. |
| [admin-runbooks.md](admin-runbooks.md) | Administrator playbooks: terminal commands, Hygraph edits, and agent handoffs. |
| [review-and-publication.md](review-and-publication.md) | Human review, Gold promotion, catalog generation, and R2 upload. |
| [operations.md](operations.md) | Environment variables, commands, dry runs, and troubleshooting. |

## Operating rules

1. Discord messages are stored as immutable Bronze observations. Never rewrite
   the captured text to make it editorial.
2. All draft content and review decisions live in Silver `ModDossier` records.
   A temporary input file is only a transport mechanism for one command.
3. Agents may collect evidence and propose text, but a human owns the
   `APPROVED` decision and all public releases.
4. Only published Gold `Campaign` and `CampaignRelease` records are read by
   the catalog generator.
5. Run every mutating command as a dry run first. `--apply` is explicit; R2
   upload is a separate explicit `--upload` step.

## Current delivery boundary

The current production `catalog.json` contains the card fields needed by the
app: title, author, short description, tags, branch, and release metadata.
Silver also holds rich editorial text, recommendations, difficulty, and source
evidence. Those richer fields are review data today; they are not yet emitted
by `generate-production-catalog.mjs`.
