import { Client, SimpleFieldType, VisibilityTypes } from "@hygraph/management-sdk";
import { hasFlag, requireEnvironment } from "./common.mjs";

// Kept separate from free-form sourceNotes: this field is generated from
// immutable Bronze observations and can be rebuilt without overwriting a
// reviewer's own notes.
const apply = hasFlag("--apply");
const migrationName = process.env.HYGRAPH_SCHEMA_MIGRATION_NAME ?? "ccm-reborn-003-medallion-source-context-v1";
const client = new Client({
  authToken: requireEnvironment("HYGRAPH_TOKEN"),
  endpoint: requireEnvironment("HYGRAPH_HP_CONTENT_ENDPOINT"),
  managementEndpoint: requireEnvironment("HYGRAPH_MANAGEMENT_ENDPOINT"),
  name: migrationName,
});

client.createSimpleField({
  parentApiId: "ModDossier",
  apiId: "sourceContext",
  displayName: "Selected source context",
  description: "Silver: a reviewable bundle of explicitly selected immutable Bronze messages. Rebuild from source selections; do not use this field for reviewer notes.",
  type: SimpleFieldType.Richtext,
  visibility: VisibilityTypes.ReadWrite,
});

const changes = client.dryRun();
console.log(`Prepared ${changes.length} source-context schema changes.`);
console.log(JSON.stringify(changes, null, 2));

if (!apply) {
  console.log("Dry run only. Re-run with --apply after reviewing this exact migration.");
  process.exit(0);
}

const result = await client.run(true);
if (result.errors) throw new Error(`Hygraph rejected schema migration: ${JSON.stringify(result.errors)}`);
console.log(`Schema migration completed: ${result.name ?? migrationName}`);
