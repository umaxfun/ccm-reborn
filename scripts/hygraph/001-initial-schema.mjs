import { Client, RelationalFieldType, SimpleFieldType, VisibilityTypes } from "@hygraph/management-sdk";
import { hasFlag, requireEnvironment } from "./common.mjs";

const apply = hasFlag("--apply");
const migrationName = process.env.HYGRAPH_SCHEMA_MIGRATION_NAME ?? "ccm-reborn-001-initial-campaign-schema-v3";
const client = new Client({
  authToken: requireEnvironment("HYGRAPH_TOKEN"),
  endpoint: requireEnvironment("HYGRAPH_HP_CONTENT_ENDPOINT"),
  managementEndpoint: requireEnvironment("HYGRAPH_MANAGEMENT_ENDPOINT"),
  name: migrationName,
});

client.createModel({
  apiId: "Campaign",
  apiIdPlural: "Campaigns",
  displayName: "Campaign",
  description: "A StarCraft II campaign/mod displayed by CCM Reborn.",
});

client.createModel({
  apiId: "CampaignRelease",
  apiIdPlural: "CampaignReleases",
  displayName: "Campaign release",
  description: "An immutable package release stored in Cloudflare R2.",
});

client.createEnumeration({
  apiId: "CampaignBranch",
  displayName: "Campaign branch",
  values: [
    { apiId: "WINGS_OF_LIBERTY", displayName: "Wings of Liberty" },
    { apiId: "HEART_OF_THE_SWARM", displayName: "Heart of the Swarm" },
    { apiId: "LEGACY_OF_THE_VOID", displayName: "Legacy of the Void" },
    { apiId: "NOVA_COVERT_OPS", displayName: "Nova Covert Ops" },
  ],
});

const createCampaignField = (apiId, displayName, type, options = {}) => client.createSimpleField({
  parentApiId: "Campaign",
  apiId,
  displayName,
  type,
  visibility: VisibilityTypes.ReadWrite,
  ...options,
});

createCampaignField("campaignId", "Campaign ID", SimpleFieldType.String, {
  description: "Stable CCM catalog ID. Do not change after publication.",
  isRequired: true,
  isUnique: true,
});
createCampaignField("catalogOrder", "Catalog order", SimpleFieldType.Int, {
  description: "Explicit display order in the generated CCM production catalog.",
  isRequired: true,
  isUnique: true,
});
createCampaignField("title", "Title", SimpleFieldType.String, { isRequired: true, isTitle: true });
createCampaignField("author", "Author", SimpleFieldType.String, { isRequired: true });
createCampaignField("shortDescription", "Short description", SimpleFieldType.String, {
  description: "One-paragraph catalog description shown before the full editorial body.",
  isRequired: true,
});
createCampaignField("editorialBody", "Editorial body", SimpleFieldType.Richtext, {
  description: "Long-form campaign description, tips, and recommendation context.",
});
createCampaignField("tags", "Tags", SimpleFieldType.String, { isList: true, isRequired: true });
createCampaignField("chooseThisIf", "Choose this if", SimpleFieldType.String, { isList: true });
createCampaignField("avoidThisIf", "Avoid this if", SimpleFieldType.String, { isList: true });
createCampaignField("estimatedHours", "Estimated hours", SimpleFieldType.Float);
createCampaignField("languages", "Languages", SimpleFieldType.String, { isList: true });
createCampaignField("isFeatured", "Featured", SimpleFieldType.Boolean, { initialValue: "false" });
createCampaignField("featuredOrder", "Featured order", SimpleFieldType.Int);

client.createEnumerableField({
  parentApiId: "Campaign",
  apiId: "branch",
  displayName: "Campaign branch",
  enumerationApiId: "CampaignBranch",
  isRequired: true,
  visibility: VisibilityTypes.ReadWrite,
});

const createReleaseField = (apiId, displayName, type, options = {}) => client.createSimpleField({
  parentApiId: "CampaignRelease",
  apiId,
  displayName,
  type,
  visibility: VisibilityTypes.ReadWrite,
  ...options,
});

createReleaseField("releaseKey", "Release key", SimpleFieldType.String, {
  description: "Immutable campaignId@version identifier.",
  isRequired: true,
  isUnique: true,
  isTitle: true,
});
createReleaseField("version", "Version", SimpleFieldType.String, { isRequired: true });
createReleaseField("packageUrl", "Package URL", SimpleFieldType.String, { isRequired: true });
createReleaseField("packageSha256", "Package SHA-256", SimpleFieldType.String, { isRequired: true });
createReleaseField("packageSize", "Package size", SimpleFieldType.Int, { isRequired: true });

client.createRelationalField({
  parentApiId: "Campaign",
  apiId: "releases",
  displayName: "Releases",
  type: RelationalFieldType.Relation,
  isList: true,
  reverseField: {
    apiId: "campaign",
    modelApiId: "CampaignRelease",
    displayName: "Campaign",
    isList: false,
  },
});

client.createRelationalField({
  parentApiId: "Campaign",
  apiId: "currentRelease",
  displayName: "Current release",
  type: RelationalFieldType.Relation,
  isList: false,
  reverseField: {
    apiId: "currentForCampaign",
    modelApiId: "CampaignRelease",
    displayName: "Current for campaign",
    isList: false,
    isHidden: true,
  },
});

client.createRelationalField({
  parentApiId: "Campaign",
  apiId: "cover",
  displayName: "Cover",
  type: RelationalFieldType.Asset,
  isList: false,
  reverseField: {
    apiId: "campaignCoverFor",
    modelApiId: "Asset",
    displayName: "Campaign cover for",
    isList: true,
  },
});

client.createRelationalField({
  parentApiId: "Campaign",
  apiId: "screenshots",
  displayName: "Screenshots",
  type: RelationalFieldType.Asset,
  isList: true,
  reverseField: {
    apiId: "campaignScreenshotsFor",
    modelApiId: "Asset",
    displayName: "Campaign screenshots for",
    isList: true,
  },
});

const changes = client.dryRun();
console.log(`Prepared ${changes.length} schema changes.`);
console.log(JSON.stringify(changes, null, 2));

if (!apply) {
  console.log("Dry run only. Re-run with --apply after reviewing this exact migration.");
  process.exit(0);
}

const result = await client.run(true);
if (result.errors) throw new Error(`Hygraph rejected schema migration: ${JSON.stringify(result.errors)}`);
console.log(`Schema migration completed: ${result.name ?? migrationName}`);
