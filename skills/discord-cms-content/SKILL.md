---
name: discord-cms-content
description: Collect Discord evidence and prepare source-backed Silver content for CCM Reborn in Hygraph. Use when asked to add or refresh a mod source, read Discord threads that local RPC did not capture fully, select relevant messages, or draft/edit a ModDossier for human review. Do not use for approving, promoting to Gold, publishing Hygraph, or uploading the production catalog.
---

# Discord CMS content

Prepare reliable Bronze evidence and Silver drafts. Keep the human in control
of review and publication.

## Read first

Read these project documents before changing content:

- `docs/cms/architecture.md`
- `docs/cms/ingestion.md`
- `docs/cms/admin-runbooks.md`
- `docs/cms/operations.md`

## Boundaries

- Use Discord local RPC when it exposes the needed messages. Otherwise read the
  already signed-in Discord UI. Never use a copied Discord user token.
- Do not close the signed-in Discord browser tab.
- Store literal source text, URLs, authors, and message IDs in Bronze. Do not
  rewrite raw evidence as editorial copy.
- Work only in Bronze and Silver. Never set `APPROVED`, create or publish Gold
  records, upload to R2, or run `cms:generate -- --upload`.
- Run a dry run before every command that can mutate Hygraph. Apply only the
  requested Bronze/Silver operation after checking the command plan.
- Do not infer a version, download URL, status, or CCM compatibility without
  source evidence. Mark it unknown or leave it blank when uncertain.

## Workflow

1. Identify the dossier and source feed. Ask the administrator when a title
   match could be a different mod or fork.
2. Capture missing Discord messages into an explicit `work/` Browser Bronze
   input when RPC is incomplete. Include only visible literal content.
3. Import or validate Bronze data using the documented script and explicit
   input path.
4. Select origin, relevant update, and context messages by meaning, not merely
   by date or length. Materialize them in Silver only with the source-selection
   command.
5. Draft concise source-backed Silver content. Use `IN_REVIEW` only through the
   editorial-batch command; retain uncertainty in source notes or empty fields.
6. Report the changed feed IDs, observation message URLs, dossier key, proposed
   factual values, and anything that still needs a human decision.

## Editorial rules

- Distinguish source wording from editorial summary; do not claim features that
  the source does not support.
- Prefer several relevant messages in `sourceContext` when a single post cannot
  explain both the project and its current release.
- Treat reaction counts as a prioritization signal only.
- Preserve separate sources and dossiers for forks unless the administrator
  explicitly resolves them as one mod.
