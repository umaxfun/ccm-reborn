# Review and publication

## Human review in Silver

Review `ModDossier` records in Hygraph. An agent can prepare an `IN_REVIEW`
draft, but only a person sets `APPROVED`, `CHANGES_REQUESTED`, or `REJECTED`.

Before approval, confirm all of the following:

- The primary feed is official and belongs to the intended project.
- `sourceContext` contains the source description and the relevant current
  update, not merely a short forum-card preview.
- A separate update channel is connected where appropriate.
- Version and download URL are supported by the selected evidence.
- Author status and CCM compatibility state facts rather than assumptions.
- Copy, tags, difficulty, and recommendation guidance match the evidence.
- The record is not a duplicate of an existing mod or a distinct fork.

For an uncertain fact, leave the value unknown or empty and request the missing
evidence. Reactions help prioritize review; they do not prove correctness.

## Promote approved content to Gold

There is currently no automated Silver-to-Gold promotion command. The human
gate therefore includes a deliberate Hygraph operation:

1. Create or update the draft `Campaign` with the stable campaign ID and order.
2. Copy the approved public card fields: title, author, short description,
   tags, and branch.
3. Create a `CampaignRelease` only for a validated R2 package. It requires a
   version, unique HTTPS URL, SHA-256, and byte size.
4. Set `Campaign.currentRelease`, verify `ModDossier.goldCampaign`, then
   publish both the release and campaign in Hygraph.

Rich editorial fields are not currently included in `catalog.json`.

## Generate and upload the app catalog

Generate locally first:

```sh
npm run cms:generate
npm run cms:verify-local
```

The generated file is `work/hygraph-catalog.json`. Inspect its diff: campaign
count, descriptions, tags, versions, URLs, checksums, and sizes. The generator
blocks an implicit campaign removal. An intentional removal must be named:

```sh
npm run cms:generate -- --allow-remove obsolete-mod
```

Only then upload the validated result:

```sh
npm run cms:generate -- --upload
npm run catalog:smoke
```

With `--upload`, the generator writes an immutable history object before
updating the public `catalog.json`. It compares against the remote catalog and
refuses unsafe release metadata changes.

## Failure handling

| Situation | Correct response |
| --- | --- |
| Wrong Discord post was selected | Capture the correct message in Bronze, update the Silver selection, and retain the old observation. |
| Editorial draft is inaccurate | Set `CHANGES_REQUESTED`, record the specific issue, and submit a revised source-backed draft. |
| Package is unverified | Do not create or publish the Gold release. |
| Generator reports a removal | Restore the Campaign or explicitly approve that exact ID with `--allow-remove`. |
| R2 history key conflicts | Stop and inspect the generated data; the conflict prevents overwriting a different immutable snapshot. |
