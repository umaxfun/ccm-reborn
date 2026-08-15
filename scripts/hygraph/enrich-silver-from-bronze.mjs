import { flagValues, graphQlRequest, hasFlag, requireEnvironment } from "./common.mjs";

// First-pass Silver enrichment from an immutable Bronze forum-card observation.
// It intentionally makes only source-derived draft fields: a short excerpt,
// conservative feature tags, and a clearly labelled download/version extraction.
// It never writes a review decision, Gold record, or publication stage.  The
// reviewer still decides whether this is suitable public copy and whether the
// linked download is a supported package.
const apply = hasFlag("--apply");
const force = hasFlag("--force");
const limit = numberFlag("--limit", Number.POSITIVE_INFINITY);
const offset = numberFlag("--offset", 0);

assert(limit > 0, "--limit must be positive.");
assert(offset >= 0, "--offset must not be negative.");

const endpoint = requireEnvironment("HYGRAPH_CONTENT_ENDPOINT");
const token = requireEnvironment("HYGRAPH_TOKEN");
const dossiers = await readGeneratedDossiers();
const ready = dossiers
  .map(toEnrichment)
  .filter((item) => force || !item.existingShortDescription)
  .sort((left, right) => left.dossierKey.localeCompare(right.dossierKey));
const selected = ready.slice(offset, Number.isFinite(limit) ? offset + limit : undefined);

console.log(`Generated forum dossiers read: ${dossiers.length}; ready for first-pass enrichment: ${ready.length}.`);
console.log(`Selected ${selected.length} records (offset ${offset}${Number.isFinite(limit) ? `, limit ${limit}` : ""}).`);
for (const item of selected.slice(0, 10)) {
  console.log(`${item.dossierKey}: ${item.shortDescription}`);
}
if (selected.length > 10) console.log(`… plus ${selected.length - 10} more.`);

if (!apply) {
  console.log("Dry run only. No Silver fields, Gold Campaigns, production JSON, or publication will be changed.");
  process.exit(0);
}

let lastMutationStartedAt = 0;
for (const item of selected) {
  await contentMutation(`
    mutation EnrichModDossier($where: ModDossierWhereUniqueInput!, $data: ModDossierUpdateInput!) {
      updateModDossier(where: $where, data: $data) { dossierKey }
    }
  `, { where: { dossierKey: item.dossierKey }, data: item.update });
  console.log(`Enriched Silver dossier: ${item.dossierKey}`);
}
console.log(`Enriched ${selected.length} draft Silver dossiers from their Bronze forum-card snapshots. Nothing was published or promoted to Gold.`);

async function contentMutation(query, variables) {
  const waitMs = Math.max(0, 260 - (Date.now() - lastMutationStartedAt));
  if (waitMs) await new Promise((resolvePromise) => setTimeout(resolvePromise, waitMs));
  lastMutationStartedAt = Date.now();
  return graphQlRequest({ endpoint, token, query, variables });
}

async function readGeneratedDossiers() {
  const dossiers = [];
  let after;
  do {
    const result = await graphQlRequest({
      endpoint,
      token,
      query: `
        query GeneratedForumDossiers($after: String) {
          modDossiersConnection(stage: DRAFT, first: 100, after: $after, where: { dossierKey_starts_with: "discord-forum-" }) {
            pageInfo { hasNextPage endCursor }
            edges {
              node {
                dossierKey
                title
                author
                branch
                authorStatus
                shortDescription
                originDescriptionObservation { rawText { raw } }
              }
            }
          }
        }
      `,
      variables: { after },
    });
    dossiers.push(...result.modDossiersConnection.edges.map((edge) => edge.node));
    after = result.modDossiersConnection.pageInfo.hasNextPage
      ? result.modDossiersConnection.pageInfo.endCursor
      : undefined;
  } while (after);
  return dossiers;
}

function toEnrichment(dossier) {
  const source = parseSourceCard(plainText(dossier.originDescriptionObservation?.rawText?.raw));
  const inferredTags = featureTags(`${dossier.title}\n${source.description}`);
  const shortDescription = sourceExcerpt(source.description, dossier);
  const update = stripUndefined({
    shortDescription,
    tags: [...new Set([branchLabel(dossier.branch), ...inferredTags].filter(Boolean))],
    latestKnownVersion: source.version,
    downloadUrl: source.downloadUrl,
  });
  return { dossierKey: dossier.dossierKey, existingShortDescription: dossier.shortDescription, shortDescription, update };
}

function parseSourceCard(raw) {
  const descriptionStart = raw.indexOf("Visible description:");
  const afterDescription = descriptionStart >= 0 ? raw.slice(descriptionStart + "Visible description:".length).trim() : raw;
  // The card snapshot appends its Discord timestamp. It is metadata rather than
  // editorial content and should not leak into public draft copy.
  const description = afterDescription.replace(/\n(?:Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday), [^\n]+$/i, "").trim();
  const downloadMatch = description.match(/\b(?:download(?:\s+link|\s+here)?|installation)\s*:?\s*(https?:\/\/[^\s)>\]]+)/i);
  const versionMatch = `${raw}`.match(/\b(?:version|current\s+version|patch(?:\s+notes)?\s+(?:for\s+)?)\s*(?:v\.?\s*)?(\d+(?:\.\d+)+(?:[-._a-z0-9]+)?)/i)
    ?? `${raw}`.match(/\bv(\d+(?:\.\d+)+(?:[-._a-z0-9]+)?)/i);
  return {
    description,
    downloadUrl: downloadMatch?.[1],
    version: versionMatch?.[1],
  };
}

function sourceExcerpt(description, dossier) {
  const sentences = description
    .replace(/https?:\/\/[^\s)>\]]+/g, "")
    .replace(/\b(?:download(?:\s+link|\s+here)?|installation)\s*:?/gi, "")
    .replace(/^\s*link to original thread[\s\S]*?\b(?=a mod that)/i, "")
    .replace(/^\s*spreadsheet containing all the units used\s*:\s*/i, "")
    .replace(/\s+/g, " ")
    .split(/(?<=[.!?])\s+/)
    .map((sentence) => sentence.trim())
    .filter((sentence) => sentence.length >= 35 && !/^(?:thanks|cheats?|screenshots?|patch notes?|report bugs?|roadmap|spreadsheet)\b/i.test(sentence));
  const relevant = [...sentences].sort((left, right) => sentenceScore(right) - sentenceScore(left))[0];
  if (!relevant || sentenceScore(relevant) < 2) return fallbackDescription(dossier, description);
  return truncate(relevant, 320);
}

function sentenceScore(sentence) {
  let score = 0;
  if (/\b(?:adds?|featur(?:es|ing)|introduces?|imports?|randomi[sz]|rework|rebalanc|command|uses|built around)\b/i.test(sentence)) score += 5;
  if (/\b(?:new units?|upgrades?|missions?|hero|tech tree|allies|races?|difficulty)\b/i.test(sentence)) score += 3;
  if (/^(?:alright|well|formerly|this post|made by|link to|spreadsheet)/i.test(sentence)) score -= 6;
  if (/\b(?:thank|cheat|support me|bug report|changelog|patch notes?)\b/i.test(sentence)) score -= 5;
  return score;
}

function fallbackDescription(dossier, description) {
  const branch = branchLabel(dossier.branch) ?? "StarCraft II";
  const source = description.toLowerCase();
  const qualifiers = [
    [/realism/, "a realism-focused redesign"],
    [/nightmare|difficulty/, "a difficulty-focused redesign"],
    [/randomi[sz]/, "a campaign randomizer"],
    [/rogue\s*\(?like\)?/, "a roguelike-style run"],
    [/story|timeline|narrative|writing/, "a story-focused reinterpretation"],
  ];
  const qualifier = qualifiers.find(([pattern]) => pattern.test(source))?.[1] ?? "a community campaign mod";
  return `A ${statusLabel(dossier.authorStatus)} ${branch} mod by ${dossier.author ?? "an unknown author"}, presented by its author as ${qualifier}.`;
}

function featureTags(value) {
  const source = value.toLowerCase();
  const tags = [];
  if (/rogue\s*\(?like\)?/.test(source)) tags.push("Roguelike", "Replayable");
  if (/randomi[sz]/.test(source)) tags.push("Randomizer", "Replayable");
  if (/nightmare|difficulty|harder than brutal/.test(source)) tags.push("Difficulty Mod");
  if (/\bco[- ]?op\b|ai ally|allies/.test(source)) tags.push("Co-op / AI Allies");
  if (/new (?:unit|units|unit tree)|custom units|rebalanced units/.test(source)) tags.push("Custom Units");
  if (/upgrade|armory|tech tree|merc/.test(source)) tags.push("Upgrades & Tech");
  if (/reworked missions|mission rework|altered missions|new missions/.test(source)) tags.push("Mission Rework");
  if (/story|narrative|writing|timeline|lore/.test(source)) tags.push("Story Rewrite");
  if (/hero unit|hero character|playable hero/.test(source)) tags.push("Hero Focus");
  if (/all 3 races|three races|3 race/.test(source)) tags.push("Three Races");
  return tags;
}

function branchLabel(branch) {
  return {
    WINGS_OF_LIBERTY: "Wings of Liberty",
    HEART_OF_THE_SWARM: "Heart of the Swarm",
    LEGACY_OF_THE_VOID: "Legacy of the Void",
    NOVA_COVERT_OPS: "Nova Covert Ops",
  }[branch];
}

function statusLabel(status) {
  return status === "COMPLETE" ? "completed" : status === "IN_PROGRESS" ? "in-progress" : "community";
}

function truncate(value, limit) {
  if (value.length <= limit) return value;
  const ending = value.lastIndexOf(" ", limit - 1);
  return `${value.slice(0, ending > 80 ? ending : limit - 1).trimEnd()}…`;
}

function plainText(value) {
  if (!value || typeof value !== "object") return "";
  const text = typeof value.text === "string" ? [value.text] : [];
  const children = Array.isArray(value.children) ? value.children.flatMap((child) => plainText(child)) : [];
  return [...text, ...children].join("\n");
}

function stripUndefined(value) {
  return Object.fromEntries(Object.entries(value).filter(([, item]) => item !== undefined && item !== null && item !== ""));
}

function numberFlag(flag, fallback) {
  const values = flagValues(flag);
  assert(values.length <= 1, `${flag} may be specified once.`);
  if (!values.length) return fallback;
  const value = Number(values[0]);
  assert(Number.isSafeInteger(value), `${flag} must be an integer.`);
  return value;
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
