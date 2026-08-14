import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const catalogPath = resolve(process.cwd(), "catalog/catalog.json");
const editorialPath = resolve(process.cwd(), "scripts/hygraph/campaign-editorial.json");
const sourceMapPath = resolve(process.cwd(), "scripts/discord/campaign-source-map.json");
const difficultyValues = new Set(["Low", "Flexible", "Variable", "Challenging", "High", "Extreme", "Unknown"]);

async function readJson(path) {
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    throw new Error(`Could not read ${path}: ${error.message}`);
  }
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function countBy(values) {
  return Object.fromEntries([...values].sort().map((value) => [value, 0]));
}

const [catalog, editorial, sourceMap] = await Promise.all([
  readJson(catalogPath),
  readJson(editorialPath),
  readJson(sourceMapPath),
]);

assert(catalog.format === 1 && Array.isArray(catalog.campaigns), `${catalogPath} is not a format-1 catalog.`);
assert(editorial.format === 1 && editorial.tagTaxonomy && Array.isArray(editorial.campaigns), `${editorialPath} is not a format-1 editorial document.`);
assert(sourceMap.format === 1 && Array.isArray(sourceMap.mappings), `${sourceMapPath} is not a format-1 source map.`);

const taxonomy = new Set(Object.keys(editorial.tagTaxonomy));
for (const [tag, description] of Object.entries(editorial.tagTaxonomy)) {
  assert(typeof description === "string" && description.trim(), `Taxonomy tag ${tag} needs a description.`);
}

const catalogById = new Map(catalog.campaigns.map((campaign) => [campaign.id, campaign]));
const sourceCampaignIds = new Set(sourceMap.mappings.map((mapping) => mapping.campaignId));
const seenIds = new Set();
const difficultyCounts = countBy(difficultyValues);
const styleCounts = new Map();

for (const entry of editorial.campaigns) {
  const prefix = `Editorial entry ${entry?.campaignId ?? "<unknown>"}`;
  assert(typeof entry?.campaignId === "string" && catalogById.has(entry.campaignId), `${prefix} is not in the production catalog.`);
  assert(!seenIds.has(entry.campaignId), `${prefix} occurs more than once.`);
  seenIds.add(entry.campaignId);
  assert(sourceCampaignIds.has(entry.campaignId), `${prefix} has no reviewed Discord source map entry.`);
  assert(typeof entry.shortDescription === "string" && entry.shortDescription.length >= 40 && entry.shortDescription.length <= 350, `${prefix} shortDescription must be 40–350 characters.`);
  assert(Array.isArray(entry.editorialBody) && entry.editorialBody.length >= 2 && entry.editorialBody.every((paragraph) => typeof paragraph === "string" && paragraph.trim()), `${prefix} needs at least two editorial paragraphs.`);
  assert(Array.isArray(entry.tags) && entry.tags.length >= 2, `${prefix} needs at least two controlled tags.`);
  assert(entry.tags.includes(catalogById.get(entry.campaignId).requirements.campaign), `${prefix} must retain its campaign branch tag.`);
  for (const tag of entry.tags) assert(taxonomy.has(tag), `${prefix} uses unknown tag: ${tag}.`);
  assert(entry.classification && Array.isArray(entry.classification.styles), `${prefix} needs classification.styles.`);
  assert(difficultyValues.has(entry.classification.difficulty), `${prefix} has invalid difficulty: ${entry.classification.difficulty}.`);
  difficultyCounts[entry.classification.difficulty] += 1;
  for (const style of entry.classification.styles) {
    assert(taxonomy.has(style), `${prefix} uses unknown style: ${style}.`);
    assert(entry.tags.includes(style), `${prefix} style ${style} must also be a tag.`);
    styleCounts.set(style, (styleCounts.get(style) ?? 0) + 1);
  }
  for (const field of ["chooseThisIf", "avoidThisIf"]) {
    assert(Array.isArray(entry[field]) && entry[field].length >= 2 && entry[field].every((item) => typeof item === "string" && item.trim()), `${prefix} needs at least two ${field} entries.`);
  }
}

const missing = [...catalogById.keys()].filter((id) => !seenIds.has(id));
assert(missing.length === 0, `Editorial document is missing ${missing.length} campaigns: ${missing.join(", ")}`);
assert(seenIds.size === catalogById.size, `Editorial document has ${seenIds.size} entries for ${catalogById.size} catalog campaigns.`);

console.log(`Validated editorial copy and classification for ${seenIds.size} campaigns.`);
console.log(`Difficulty: ${Object.entries(difficultyCounts).filter(([, count]) => count).map(([value, count]) => `${value} ${count}`).join(", ")}.`);
console.log(`Styles: ${[...styleCounts.entries()].sort(([left], [right]) => left.localeCompare(right)).map(([value, count]) => `${value} ${count}`).join(", ")}.`);
