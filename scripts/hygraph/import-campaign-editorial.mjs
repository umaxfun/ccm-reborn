import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { graphQlRequest, hasFlag, requireEnvironment } from "./common.mjs";

const apply = hasFlag("--apply");
const publish = hasFlag("--publish");
const editorialPath = resolve(process.cwd(), "scripts/hygraph/campaign-editorial.json");

if (publish && !apply) throw new Error("--publish requires --apply.");

// This module performs the full 38-entry and controlled-taxonomy validation
// before it ever reads CMS credentials or sends a mutation.
await import("./check-campaign-editorial.mjs");
const editorial = JSON.parse(await readFile(editorialPath, "utf8"));

function richTextFromParagraphs(paragraphs) {
  return {
    children: paragraphs.map((text) => ({
      type: "paragraph",
      children: [{ text }],
    })),
  };
}

if (!apply) {
  console.log(`Dry run only. ${editorial.campaigns.length} campaign editorial records are ready.`);
  console.log("Re-run with --apply to update drafts, then add --publish to make them visible in the published API.");
  process.exit(0);
}

const endpoint = requireEnvironment("HYGRAPH_CONTENT_ENDPOINT");
const token = requireEnvironment("HYGRAPH_TOKEN");
const updateCampaign = `
  mutation UpdateCampaignEditorial($where: CampaignWhereUniqueInput!, $data: CampaignUpdateInput!) {
    updateCampaign(where: $where, data: $data) { id campaignId }
  }
`;
const publishCampaign = `
  mutation PublishCampaignEditorial($where: CampaignWhereUniqueInput!) {
    publishCampaign(where: $where, to: PUBLISHED) { id campaignId }
  }
`;

let lastMutationStartedAt = 0;
async function contentMutation(query, variables) {
  const waitMs = Math.max(0, 260 - (Date.now() - lastMutationStartedAt));
  if (waitMs) await new Promise((resolvePromise) => setTimeout(resolvePromise, waitMs));
  lastMutationStartedAt = Date.now();
  return graphQlRequest({ endpoint, token, query, variables });
}

for (const entry of editorial.campaigns) {
  await contentMutation(updateCampaign, {
    where: { campaignId: entry.campaignId },
    data: {
      shortDescription: entry.shortDescription,
      editorialBody: richTextFromParagraphs(entry.editorialBody),
      tags: entry.tags,
      chooseThisIf: entry.chooseThisIf,
      avoidThisIf: entry.avoidThisIf,
    },
  });
  console.log(`Updated editorial draft: ${entry.campaignId}.`);
}

if (!publish) {
  console.log("Updated editorial drafts only; they are not published. Re-run with --apply --publish after review.");
  process.exit(0);
}

for (const entry of editorial.campaigns) {
  await contentMutation(publishCampaign, { where: { campaignId: entry.campaignId } });
  console.log(`Published editorial: ${entry.campaignId}.`);
}

console.log(`Updated and published editorial content for ${editorial.campaigns.length} campaigns.`);
