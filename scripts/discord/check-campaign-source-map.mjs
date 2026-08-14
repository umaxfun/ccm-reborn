import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const catalogPath = resolve(process.cwd(), "catalog/catalog.json");
const mapPath = resolve(process.cwd(), "scripts/discord/campaign-source-map.json");
const sourceMap = JSON.parse(await readFile(mapPath, "utf8"));
const catalog = JSON.parse(await readFile(catalogPath, "utf8"));
const allowedEvidence = new Set(["content-verified", "name-match", "shared-reference"]);
const allowedSourceKinds = new Set(["channel", "forum-thread"]);

if (sourceMap.format !== 1) throw new Error(`${mapPath} must use format 1.`);
if (!/^\d{17,20}$/.test(sourceMap.guildId ?? "")) throw new Error("Source map guildId must be a Discord snowflake.");
if (!Array.isArray(sourceMap.mappings)) throw new Error("Source map mappings must be an array.");

const catalogIds = new Set(catalog.campaigns.map((campaign) => campaign.id));
const mappedIds = new Set();
for (const mapping of sourceMap.mappings) {
  if (!catalogIds.has(mapping.campaignId)) throw new Error(`Unknown campaign ID in source map: ${mapping.campaignId}.`);
  if (mappedIds.has(mapping.campaignId)) throw new Error(`Duplicate campaign ID in source map: ${mapping.campaignId}.`);
  mappedIds.add(mapping.campaignId);
  if (!/^\d{17,20}$/.test(mapping.channelId ?? "")) throw new Error(`Invalid Discord channel ID for ${mapping.campaignId}.`);
  if (mapping.messageId && !/^\d{17,20}$/.test(mapping.messageId)) throw new Error(`Invalid Discord message ID for ${mapping.campaignId}.`);
  if (!allowedSourceKinds.has(mapping.sourceKind)) throw new Error(`Invalid source kind for ${mapping.campaignId}.`);
  if (!allowedEvidence.has(mapping.evidence)) throw new Error(`Invalid evidence value for ${mapping.campaignId}.`);
}

const missing = [...catalogIds].filter((id) => !mappedIds.has(id));
const unexpected = [...mappedIds].filter((id) => !catalogIds.has(id));
if (missing.length || unexpected.length) {
  throw new Error(`Source map coverage mismatch. Missing: ${missing.join(", ") || "none"}. Unexpected: ${unexpected.join(", ") || "none"}.`);
}

const evidenceCounts = sourceMap.mappings.reduce((counts, mapping) => {
  counts[mapping.evidence] += 1;
  return counts;
}, { "content-verified": 0, "name-match": 0, "shared-reference": 0 });
console.log(`Validated ${sourceMap.mappings.length} campaign sources: ${evidenceCounts["content-verified"]} content-verified, ${evidenceCounts["name-match"]} name-match, ${evidenceCounts["shared-reference"]} shared-reference.`);
