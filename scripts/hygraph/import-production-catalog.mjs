import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { catalogCampaignToCmsData, validateCatalog } from "./catalog-format.mjs";
import { graphQlRequest, hasFlag, requireEnvironment } from "./common.mjs";

const apply = hasFlag("--apply");
const publish = !hasFlag("--no-publish");
const sourcePath = resolve(process.cwd(), "catalog/catalog.json");
const catalog = JSON.parse(await readFile(sourcePath, "utf8"));
validateCatalog(catalog);

if (!apply) {
  console.log(`Validated ${catalog.campaigns.length} production campaigns from ${sourcePath}.`);
  console.log("Dry run only. Re-run with --apply to upsert drafts and publish them.");
  process.exit(0);
}

const endpoint = requireEnvironment("HYGRAPH_CONTENT_ENDPOINT");
const token = requireEnvironment("HYGRAPH_TOKEN");
const upsertCampaign = `
  mutation UpsertCampaign($where: CampaignWhereUniqueInput!, $upsert: CampaignUpsertInput!) {
    upsertCampaign(where: $where, upsert: $upsert) { id campaignId }
  }
`;
const upsertRelease = `
  mutation UpsertCampaignRelease($where: CampaignReleaseWhereUniqueInput!, $upsert: CampaignReleaseUpsertInput!) {
    upsertCampaignRelease(where: $where, upsert: $upsert) { id releaseKey }
  }
`;
const setCurrentRelease = `
  mutation SetCurrentRelease($where: CampaignWhereUniqueInput!, $data: CampaignUpdateInput!) {
    updateCampaign(where: $where, data: $data) { id campaignId }
  }
`;
const publishRelease = `
  mutation PublishCampaignRelease($where: CampaignReleaseWhereUniqueInput!) {
    publishCampaignRelease(where: $where, to: PUBLISHED) { id }
  }
`;
const publishCampaign = `
  mutation PublishCampaign($where: CampaignWhereUniqueInput!) {
    publishCampaign(where: $where, to: PUBLISHED) { id }
  }
`;

let lastMutationStartedAt = 0;
async function contentMutation(query, variables) {
  const waitMs = Math.max(0, 260 - (Date.now() - lastMutationStartedAt));
  if (waitMs) await new Promise((resolvePromise) => setTimeout(resolvePromise, waitMs));
  lastMutationStartedAt = Date.now();
  return graphQlRequest({ endpoint, token, query, variables });
}

for (const [catalogOrder, campaign] of catalog.campaigns.entries()) {
  const campaignData = catalogCampaignToCmsData(campaign, catalogOrder);
  const releaseKey = `${campaign.id}@${campaign.version}`;
  const releaseData = {
    releaseKey,
    version: campaign.version,
    packageUrl: campaign.package.url,
    packageSha256: campaign.package.sha256,
    packageSize: campaign.package.size,
    campaign: { connect: { campaignId: campaign.id } },
  };

  await contentMutation(
    upsertCampaign,
    { where: { campaignId: campaign.id }, upsert: { create: campaignData, update: campaignData } },
  );
  await contentMutation(
    upsertRelease,
    { where: { releaseKey }, upsert: { create: releaseData, update: releaseData } },
  );
  await contentMutation(
    setCurrentRelease,
    {
      where: { campaignId: campaign.id },
      data: { currentRelease: { connect: { releaseKey } } },
    },
  );
  console.log(`Upserted ${campaign.id} (${campaign.version}).`);
}

if (!publish) {
  console.log("Imported drafts without publishing them.");
  process.exit(0);
}

for (const campaign of catalog.campaigns) {
  const releaseKey = `${campaign.id}@${campaign.version}`;
  await contentMutation(publishRelease, { where: { releaseKey } });
  await contentMutation(publishCampaign, { where: { campaignId: campaign.id } });
  console.log(`Published ${campaign.id}.`);
}

console.log(`Imported and published ${catalog.campaigns.length} campaigns.`);
