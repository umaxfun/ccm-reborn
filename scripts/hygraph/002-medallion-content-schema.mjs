import { Client, RelationalFieldType, SimpleFieldType, VisibilityTypes } from "@hygraph/management-sdk";
import { hasFlag, requireEnvironment } from "./common.mjs";

const apply = hasFlag("--apply");
const migrationName = process.env.HYGRAPH_SCHEMA_MIGRATION_NAME ?? "ccm-reborn-002-medallion-content-schema-v1";
const client = new Client({
  authToken: requireEnvironment("HYGRAPH_TOKEN"),
  endpoint: requireEnvironment("HYGRAPH_HP_CONTENT_ENDPOINT"),
  managementEndpoint: requireEnvironment("HYGRAPH_MANAGEMENT_ENDPOINT"),
  name: migrationName,
});

client.createModel({
  apiId: "SourceFeed",
  apiIdPlural: "SourceFeeds",
  displayName: "Source feed",
  description: "Bronze: a reviewed official source location, such as a Discord forum post or dedicated update channel.",
});
client.createModel({
  apiId: "SourceObservation",
  apiIdPlural: "SourceObservations",
  displayName: "Source observation",
  description: "Bronze: an append-only capture of one source message at a specific time.",
});
client.createModel({
  apiId: "ModDossier",
  apiIdPlural: "ModDossiers",
  displayName: "Mod dossier",
  description: "Silver: normalized technical state and reviewable public content for a potential or published campaign.",
});

const createEnumeration = (apiId, displayName, values) => client.createEnumeration({ apiId, displayName, values });
createEnumeration("SourceFeedKind", "Source feed kind", [
  { apiId: "DISCORD_FORUM_THREAD", displayName: "Discord forum thread" },
  { apiId: "DISCORD_DEDICATED_CHANNEL", displayName: "Discord dedicated channel" },
]);
createEnumeration("CheckCadence", "Check cadence", [
  { apiId: "WEEKLY", displayName: "Weekly" },
  { apiId: "QUARTERLY", displayName: "Quarterly" },
  { apiId: "MANUAL", displayName: "Manual" },
]);
createEnumeration("AuthorStatus", "Author release status", [
  { apiId: "IN_PROGRESS", displayName: "In progress" },
  { apiId: "COMPLETE", displayName: "Complete" },
  { apiId: "PAUSED", displayName: "Paused" },
  { apiId: "ARCHIVED", displayName: "Archived" },
  { apiId: "UNKNOWN", displayName: "Unknown" },
]);
createEnumeration("CcmCompatibility", "CCM compatibility", [
  { apiId: "UNKNOWN", displayName: "Unknown" },
  { apiId: "DECLARED_COMPATIBLE", displayName: "Declared compatible" },
  { apiId: "VALIDATED", displayName: "Validated" },
  { apiId: "NOT_SUPPORTED", displayName: "Not supported" },
]);
createEnumeration("ReviewStatus", "Editorial review status", [
  { apiId: "DRAFT", displayName: "Draft" },
  { apiId: "IN_REVIEW", displayName: "In review" },
  { apiId: "CHANGES_REQUESTED", displayName: "Changes requested" },
  { apiId: "APPROVED", displayName: "Approved" },
  { apiId: "REJECTED", displayName: "Rejected" },
]);
createEnumeration("SourceEvidence", "Source evidence", [
  { apiId: "CONTENT_VERIFIED", displayName: "Content verified" },
  { apiId: "NAME_MATCH", displayName: "Name match" },
  { apiId: "SHARED_REFERENCE", displayName: "Shared reference" },
]);

const createSimpleField = (parentApiId, apiId, displayName, type, options = {}) => client.createSimpleField({
  parentApiId,
  apiId,
  displayName,
  type,
  visibility: VisibilityTypes.ReadWrite,
  ...options,
});
const createEnumerableField = (parentApiId, apiId, displayName, enumerationApiId, options = {}) => client.createEnumerableField({
  parentApiId,
  apiId,
  displayName,
  enumerationApiId,
  visibility: VisibilityTypes.ReadWrite,
  ...options,
});

createSimpleField("SourceFeed", "sourceKey", "Source key", SimpleFieldType.String, {
  description: "Stable external-source identity, for example discord:967…:152….",
  isRequired: true,
  isUnique: true,
});
createSimpleField("SourceFeed", "title", "Title", SimpleFieldType.String, { isRequired: true, isTitle: true });
createSimpleField("SourceFeed", "sourceUrl", "Source URL", SimpleFieldType.String, { isRequired: true });
createSimpleField("SourceFeed", "guildId", "Discord guild ID", SimpleFieldType.String);
createSimpleField("SourceFeed", "channelId", "Discord channel or thread ID", SimpleFieldType.String);
createSimpleField("SourceFeed", "isOfficial", "Official source", SimpleFieldType.Boolean, { initialValue: "false" });
createSimpleField("SourceFeed", "isActive", "Active", SimpleFieldType.Boolean, { initialValue: "true" });
createSimpleField("SourceFeed", "lastCheckedAt", "Last checked at", SimpleFieldType.Datetime);
createSimpleField("SourceFeed", "nextCheckAt", "Next check at", SimpleFieldType.Datetime);
createSimpleField("SourceFeed", "notes", "Notes", SimpleFieldType.Richtext);
createEnumerableField("SourceFeed", "kind", "Kind", "SourceFeedKind", { isRequired: true });
createEnumerableField("SourceFeed", "cadence", "Check cadence", "CheckCadence", { isRequired: true });

createSimpleField("SourceObservation", "observationKey", "Observation key", SimpleFieldType.String, {
  description: "Immutable source-message capture identity: sourceKey:messageId:capturedAt.",
  isRequired: true,
  isUnique: true,
  isTitle: true,
});
createSimpleField("SourceObservation", "messageId", "Message ID", SimpleFieldType.String, { isRequired: true });
createSimpleField("SourceObservation", "messageUrl", "Message URL", SimpleFieldType.String, { isRequired: true });
createSimpleField("SourceObservation", "rawText", "Raw source text", SimpleFieldType.Richtext, {
  description: "Captured source text. Do not edit; create a later observation instead.",
  isRequired: true,
});
createSimpleField("SourceObservation", "authorName", "Source author", SimpleFieldType.String);
createSimpleField("SourceObservation", "messageCreatedAt", "Message created at", SimpleFieldType.Datetime);
createSimpleField("SourceObservation", "messageEditedAt", "Message edited at", SimpleFieldType.Datetime);
createSimpleField("SourceObservation", "capturedAt", "Captured at", SimpleFieldType.Datetime, { isRequired: true });
createSimpleField("SourceObservation", "contentFingerprint", "Content fingerprint", SimpleFieldType.String, { isRequired: true });
createSimpleField("SourceObservation", "reactionCount", "Observed reaction count", SimpleFieldType.Int);
createSimpleField("SourceObservation", "reactionObservedAt", "Reactions observed at", SimpleFieldType.Datetime);
createSimpleField("SourceObservation", "reactions", "Observed reactions", SimpleFieldType.Json);

createSimpleField("ModDossier", "dossierKey", "Dossier key", SimpleFieldType.String, {
  description: "Stable Silver-layer ID; never reuse it for a different mod or fork.",
  isRequired: true,
  isUnique: true,
});
createSimpleField("ModDossier", "title", "Public title", SimpleFieldType.String, { isRequired: true, isTitle: true });
createSimpleField("ModDossier", "proposedCampaignId", "Proposed campaign ID", SimpleFieldType.String, {
  description: "Gold Campaign.campaignId to create or update after approval.",
});
createSimpleField("ModDossier", "author", "Author", SimpleFieldType.String);
createSimpleField("ModDossier", "shortDescription", "Short description", SimpleFieldType.String);
createSimpleField("ModDossier", "editorialBody", "Editorial body", SimpleFieldType.Richtext);
createSimpleField("ModDossier", "tags", "Tags", SimpleFieldType.String, { isList: true });
createSimpleField("ModDossier", "chooseThisIf", "Choose this if", SimpleFieldType.String, { isList: true });
createSimpleField("ModDossier", "avoidThisIf", "Avoid this if", SimpleFieldType.String, { isList: true });
createSimpleField("ModDossier", "latestKnownVersion", "Latest known version", SimpleFieldType.String);
createSimpleField("ModDossier", "downloadUrl", "Latest download URL", SimpleFieldType.String);
createSimpleField("ModDossier", "lastCheckedAt", "Last checked at", SimpleFieldType.Datetime);
createSimpleField("ModDossier", "nextCheckAt", "Next check at", SimpleFieldType.Datetime);
createSimpleField("ModDossier", "popularitySnapshot", "Observed reaction count", SimpleFieldType.Int);
createSimpleField("ModDossier", "popularityObservedAt", "Reactions observed at", SimpleFieldType.Datetime);
createSimpleField("ModDossier", "sourceNotes", "Source and review notes", SimpleFieldType.Richtext);
createEnumerableField("ModDossier", "branch", "Campaign branch", "CampaignBranch");
createEnumerableField("ModDossier", "sourceEvidence", "Primary source evidence", "SourceEvidence");
createEnumerableField("ModDossier", "authorStatus", "Author release status", "AuthorStatus", { isRequired: true });
createEnumerableField("ModDossier", "ccmCompatibility", "CCM compatibility", "CcmCompatibility", { isRequired: true });
createEnumerableField("ModDossier", "reviewStatus", "Editorial review status", "ReviewStatus", { isRequired: true });

client.createRelationalField({
  parentApiId: "SourceFeed",
  apiId: "observations",
  displayName: "Observations",
  type: RelationalFieldType.Relation,
  isList: true,
  reverseField: {
    apiId: "sourceFeed",
    modelApiId: "SourceObservation",
    displayName: "Source feed",
    isList: false,
  },
});
client.createRelationalField({
  parentApiId: "ModDossier",
  apiId: "primarySourceFeed",
  displayName: "Primary source feed",
  type: RelationalFieldType.Relation,
  isList: false,
  reverseField: {
    apiId: "primarySourceFor",
    modelApiId: "SourceFeed",
    displayName: "Primary source for",
    isList: true,
    isHidden: true,
  },
});
client.createRelationalField({
  parentApiId: "ModDossier",
  apiId: "officialUpdateFeed",
  displayName: "Official update feed",
  type: RelationalFieldType.Relation,
  isList: false,
  reverseField: {
    apiId: "officialUpdatesFor",
    modelApiId: "SourceFeed",
    displayName: "Official updates for",
    isList: true,
    isHidden: true,
  },
});
client.createRelationalField({
  parentApiId: "ModDossier",
  apiId: "originDescriptionObservation",
  displayName: "Origin description observation",
  type: RelationalFieldType.Relation,
  isList: false,
  reverseField: {
    apiId: "usedAsOriginFor",
    modelApiId: "SourceObservation",
    displayName: "Used as origin for",
    isList: true,
    isHidden: true,
  },
});
client.createRelationalField({
  parentApiId: "ModDossier",
  apiId: "latestUpdateObservation",
  displayName: "Latest update observation",
  type: RelationalFieldType.Relation,
  isList: false,
  reverseField: {
    apiId: "usedAsLatestUpdateFor",
    modelApiId: "SourceObservation",
    displayName: "Used as latest update for",
    isList: true,
    isHidden: true,
  },
});
client.createRelationalField({
  parentApiId: "ModDossier",
  apiId: "goldCampaign",
  displayName: "Gold campaign",
  type: RelationalFieldType.Relation,
  isList: false,
  reverseField: {
    apiId: "silverDossier",
    modelApiId: "Campaign",
    displayName: "Silver dossier",
    isList: false,
    isHidden: true,
  },
});

const changes = client.dryRun();
console.log(`Prepared ${changes.length} medallion-schema changes.`);
console.log(JSON.stringify(changes, null, 2));

if (!apply) {
  console.log("Dry run only. Re-run with --apply after reviewing this exact migration.");
  process.exit(0);
}

const result = await client.run(true);
if (result.errors) throw new Error(`Hygraph rejected schema migration: ${JSON.stringify(result.errors)}`);
console.log(`Schema migration completed: ${result.name ?? migrationName}`);
