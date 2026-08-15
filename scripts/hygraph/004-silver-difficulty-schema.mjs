import { Client, SimpleFieldType, VisibilityTypes } from "@hygraph/management-sdk";
import { hasFlag, requireEnvironment } from "./common.mjs";

// Difficulty was part of the pre-CMS editorial classification.  It belongs in
// Silver, beside the reviewable tags and source context, rather than in an
// archived JSON input.  The field is optional so newly discovered candidates
// can remain explicitly unclassified until review.
const apply = hasFlag("--apply");
const migrationName = process.env.HYGRAPH_SCHEMA_MIGRATION_NAME ?? "ccm-reborn-004-silver-difficulty-v1";
const client = new Client({
  authToken: requireEnvironment("HYGRAPH_TOKEN"),
  endpoint: requireEnvironment("HYGRAPH_HP_CONTENT_ENDPOINT"),
  managementEndpoint: requireEnvironment("HYGRAPH_MANAGEMENT_ENDPOINT"),
  name: migrationName,
});

client.createEnumeration({
  apiId: "CampaignDifficulty",
  displayName: "Campaign difficulty",
  values: [
    { apiId: "LOW", displayName: "Low" },
    { apiId: "FLEXIBLE", displayName: "Flexible" },
    { apiId: "VARIABLE", displayName: "Variable" },
    { apiId: "CHALLENGING", displayName: "Challenging" },
    { apiId: "HIGH", displayName: "High" },
    { apiId: "EXTREME", displayName: "Extreme" },
    { apiId: "UNKNOWN", displayName: "Unknown" },
  ],
});

client.createEnumerableField({
  parentApiId: "ModDossier",
  apiId: "difficulty",
  displayName: "Difficulty",
  description: "Silver review classification. It must be backed by source context or an explicitly reviewed editorial assessment.",
  enumerationApiId: "CampaignDifficulty",
  visibility: VisibilityTypes.ReadWrite,
});

const changes = client.dryRun();
console.log(`Prepared ${changes.length} Silver-difficulty schema changes.`);
console.log(JSON.stringify(changes, null, 2));

if (!apply) {
  console.log("Dry run only. Re-run with --apply after reviewing this exact migration.");
  process.exit(0);
}

const result = await client.run(true);
if (result.errors) throw new Error(`Hygraph rejected schema migration: ${JSON.stringify(result.errors)}`);
console.log(`Schema migration completed: ${result.name ?? migrationName}`);
