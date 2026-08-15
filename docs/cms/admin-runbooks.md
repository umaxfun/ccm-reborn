# Administrator runbooks

Use these playbooks as an administrator. Each one states what to run, what to
edit in Hygraph, and when to hand the task to the `discord-cms-content` agent
skill. Run every content command without `--apply` first and inspect the plan.

## 1. Add a new mod

**Goal:** create a source-backed Silver dossier. This does not publish anything.

1. In the terminal, collect the Discord source:

   ```sh
   npm run discord:export -- --channel <channel-id> --output work/new-mod.json
   npm run cms:bronze:import-rpc -- --input work/new-mod.json
   npm run cms:bronze:import-rpc -- --input work/new-mod.json --apply
   ```

   If RPC does not include the necessary message, ask the agent: “Use
   `$discord-cms-content` to capture the complete source post for <mod> into
   Browser Bronze.” Apply the resulting explicit Browser input with
   `cms:bronze:import-browser`.

2. In Hygraph, create a `ModDossier` in `DRAFT`:

   - choose a stable `dossierKey`;
   - set title, author if known, branch, author status, and CCM compatibility;
   - connect the official `primarySourceFeed`;
   - add an `officialUpdateFeed` when release updates live elsewhere.

3. Ask the agent to read the captured evidence and prepare source selection plus
   an editorial batch. Apply both batches, then inspect the dossier in Hygraph.

**Done when:** the dossier has a primary source, selected source context, and
either remains `DRAFT` or is ready as `IN_REVIEW`.

## 2. Refresh an existing mod

**Goal:** record new source information without overwriting past evidence.

1. In Hygraph, open the dossier and copy the primary/update channel ID from the
   connected `SourceFeed`.
2. In the terminal, export and import the source again:

   ```sh
   npm run discord:export -- --channel <channel-id> --output work/refresh.json
   npm run cms:bronze:import-rpc -- --input work/refresh.json
   npm run cms:bronze:import-rpc -- --input work/refresh.json --apply
   ```

3. If the relevant announcement needs interpretation, ask the agent: “Use
   `$discord-cms-content` to review new Bronze messages for <dossierKey> and
   propose an updated source selection and Silver draft.”
4. In Hygraph, review the new `sourceContext`, version, download URL, author
   status, and copy. Update `lastCheckedAt` and `nextCheckAt`.

**Done when:** the new messages exist in Bronze and Silver explicitly points to
the relevant update. Do not edit or delete older observations.

## 3. Review and edit a content draft

**Goal:** decide whether a Silver dossier is fit to become public content.

1. In Hygraph, filter `ModDossier` by `IN_REVIEW`.
2. Read the linked `sourceContext` and open the original Discord URLs.
3. Edit in Hygraph as needed: short description, body, tags, difficulty,
   choose/avoid guidance, version, download URL, and source notes.
4. Set one review state:

   - `APPROVED` when the evidence and wording are correct;
   - `CHANGES_REQUESTED` with a concrete note when the agent must revise it;
   - `REJECTED` when it should not enter the catalog.

5. For revision, ask the agent with the exact missing fact or editorial issue.
   The agent should return a new batch; do not ask it to publish anything.

**Done when:** Silver has an explicit review decision. `APPROVED` alone does
not change the app catalog.

## 4. Promote an approved mod to Gold

**Goal:** make approved card content eligible for the production catalog.

1. In Hygraph, open the approved `ModDossier` and create or update its Gold
   `Campaign` relation.
2. In the Campaign draft, set the stable `campaignId`, `catalogOrder`, title,
   author, short description, tags, and branch from the approved Silver record.
3. Create a `CampaignRelease` only after a package exists in R2 and its URL,
   version, SHA-256, and byte size are known and verified. Set it as
   `currentRelease`.
4. Verify that the dossier links to the intended Gold Campaign. Publish the
   Campaign and CampaignRelease in Hygraph.

**Done when:** both Gold records are published. There is no automatic
Silver-to-Gold command, so this is a deliberate administrator operation.

## 5. Update a published mod

Choose one path:

| Change | Administrator action |
| --- | --- |
| Description, tags, or author metadata | Refresh/review Silver first, then update and publish the Gold Campaign fields. |
| Version or package | Create a new validated `CampaignRelease`, set it as `currentRelease`, and publish it with the Campaign. |
| Both | Complete the Silver review first, then make both Gold changes as one release decision. |

The current CMS pipeline assumes the release package is already present in R2.
Uploading a ZIP to R2 and computing its metadata is not yet an integrated
CMS command; do not use a catalog-only publisher as a substitute for Gold
review.

## 6. Preview and publish the production catalog

**Goal:** publish only the current published Gold set.

1. In the terminal, generate a local candidate:

   ```sh
   npm run cms:generate
   npm run cms:verify-local
   ```

2. Inspect `work/hygraph-catalog.json` against the public catalog. Check the
   campaign count, visible descriptions/tags, releases, checksums, and sizes.
3. If a campaign was intentionally unpublished in Hygraph, explicitly permit
   that exact removal:

   ```sh
   npm run cms:generate -- --allow-remove <campaign-id>
   ```

4. Upload and smoke-test only after the preview is correct:

   ```sh
   npm run cms:generate -- --upload
   npm run catalog:smoke
   ```

**Done when:** R2 contains the new generated catalog and `catalog:smoke`
passes. `--upload` is the only step in this playbook that changes R2.

## 7. Unpublish or retire a mod

1. In Hygraph, unpublish the Gold Campaign and decide whether the Silver dossier
   should remain `APPROVED`, become `REJECTED`, or simply be marked inactive.
2. Mark inactive source feeds as needed; retain all evidence and releases.
3. Generate a preview with the exact `--allow-remove <campaign-id>` flag.
4. Inspect it, upload it, and run the smoke check.

**Done when:** the mod is absent from generated production JSON but the CMS
retains enough data to explain the decision.

## 8. Resolve a duplicate or fork

1. In Hygraph, compare the primary sources and `sourceContext` of both dossiers.
2. If they are one mod, keep one canonical dossier, reconnect the required
   source feeds, and mark the duplicate `REJECTED` with a note.
3. If they are forks, keep separate dossiers, titles, and sources. Do not merge
   them based on title similarity alone.
4. Ask the agent only to collect missing evidence; the canonical/fork decision
   belongs to the administrator.

## 9. Run a scheduled content check

1. In Hygraph, find active feeds with `nextCheckAt` in the past.
2. Process `WEEKLY` feeds first, then `QUARTERLY`; process `MANUAL` only by
   explicit decision.
3. For each source, use the refresh runbook, then set new `lastCheckedAt` and
   `nextCheckAt` values in Hygraph.
4. Use reaction counts only to prioritize the order of review, never to infer
   correctness or compatibility.
