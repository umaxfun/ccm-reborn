import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { catalogCampaignToCmsData, makeProductionCatalog, validateCatalog } from "./catalog-format.mjs";

const source = JSON.parse(await readFile(resolve(process.cwd(), "catalog/catalog.json"), "utf8"));
validateCatalog(source);

const cmsCampaigns = source.campaigns.map((campaign, catalogOrder) => ({
  ...catalogCampaignToCmsData(campaign, catalogOrder),
  currentRelease: {
    releaseKey: `${campaign.id}@${campaign.version}`,
    version: campaign.version,
    packageUrl: campaign.package.url,
    packageSha256: campaign.package.sha256,
    packageSize: campaign.package.size,
  },
}));
const regenerated = makeProductionCatalog(cmsCampaigns, source);
assert.deepEqual(regenerated, source);
console.log(`Local catalog mapping round-trip passed for ${source.campaigns.length} campaigns.`);
