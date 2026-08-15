import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { flagValues, graphQlRequest, hasFlag, requireEnvironment } from "./common.mjs";

// Promotes the Discord forum-card inventory from Bronze into deliberately
// incomplete Silver dossiers.  A forum-card preview is evidence of a project,
// not publishable copy or a validated package.  Therefore this script never
// creates/updates Gold Campaigns, publishes records, or changes production JSON.
//
// Re-runs are safe: manually edited dossier copy, review state, technical facts,
// and selected latest-update observation are left alone.  The only refreshes are
// the direct forum source relation, the immutable preview observation, and the
// recorded card popularity snapshot.
const apply = hasFlag("--apply");
const limit = numberFlag("--limit", Number.POSITIVE_INFINITY);
const offset = numberFlag("--offset", 0);
const catalogPath = resolve(process.cwd(), "catalog/catalog.json");
const forumCardMarker = "Forum-card snapshot (visible preview; open the linked thread for the complete post).";

assert(limit > 0, "--limit must be positive.");
assert(offset >= 0, "--offset must not be negative.");

const catalog = await readJson(catalogPath);
assert(catalog.format === 1 && Array.isArray(catalog.campaigns), "Catalog must be format 1.");

const endpoint = requireEnvironment("HYGRAPH_CONTENT_ENDPOINT");
const token = requireEnvironment("HYGRAPH_TOKEN");
const [forumFeeds, existingDossiers] = await Promise.all([readForumFeeds(), readDossiers()]);
const sourceAliases = aliasesFromCatalog(catalog);
const matcher = buildMatcher(existingDossiers, sourceAliases);

const candidates = forumFeeds
  .map((feed) => toPromotion(feed, matcher))
  .filter((item) => item !== null)
  .sort((left, right) => left.feed.sourceKey.localeCompare(right.feed.sourceKey));
const selected = candidates.slice(offset, Number.isFinite(limit) ? offset + limit : undefined);
const skipped = forumFeeds.length - candidates.length;
const existingMatches = candidates.filter((item) => item.match).length;
const newCandidates = candidates.length - existingMatches;

console.log(`Bronze forum feeds read: ${forumFeeds.length}; card previews selected: ${candidates.length}; skipped without a card preview: ${skipped}.`);
console.log(`Promotion plan: ${existingMatches} conservative matches to existing Silver dossiers, ${newCandidates} new draft candidates.`);
console.log(`Selected ${selected.length} records (offset ${offset}${Number.isFinite(limit) ? `, limit ${limit}` : ""}).`);
for (const item of selected.slice(0, 20)) {
  console.log(`${item.match ? "MATCH" : "NEW"} ${item.feed.title} -> ${item.dossierKey}${item.match ? ` (${item.match.reason})` : ""}`);
}
if (selected.length > 20) console.log(`… plus ${selected.length - 20} more.`);

if (!apply) {
  console.log("Dry run only. No Silver dossiers, Gold Campaigns, production JSON, or publication will be changed.");
  process.exit(0);
}

let lastMutationStartedAt = 0;
let updatedExisting = 0;
let createdCandidates = 0;
for (const item of selected) {
  const create = createInput(item);
  const update = updateInput(item);
  await contentMutation(`
    mutation UpsertModDossier($where: ModDossierWhereUniqueInput!, $upsert: ModDossierUpsertInput!) {
      upsertModDossier(where: $where, upsert: $upsert) { dossierKey }
    }
  `, {
    where: { dossierKey: item.dossierKey },
    upsert: { create, update },
  });
  if (item.match) updatedExisting += 1;
  else createdCandidates += 1;
  console.log(`${item.match ? "Linked" : "Created"} Silver dossier: ${item.dossierKey}`);
}

console.log(`Silver promotion complete: ${updatedExisting} existing dossiers linked, ${createdCandidates} new draft dossiers created.`);
console.log("Nothing was published or promoted to Gold.");

async function contentMutation(query, variables) {
  const waitMs = Math.max(0, 260 - (Date.now() - lastMutationStartedAt));
  if (waitMs) await new Promise((resolvePromise) => setTimeout(resolvePromise, waitMs));
  lastMutationStartedAt = Date.now();
  return graphQlRequest({ endpoint, token, query, variables });
}

async function readForumFeeds() {
  const feeds = [];
  let after;
  do {
    const result = await graphQlRequest({
      endpoint,
      token,
      query: `
        query ForumSourceFeeds($after: String) {
          sourceFeedsConnection(
            stage: DRAFT
            first: 100
            after: $after
            where: { kind: DISCORD_FORUM_THREAD }
          ) {
            pageInfo { hasNextPage endCursor }
            edges {
              node {
                sourceKey
                title
                sourceUrl
                channelId
                observations(first: 20) {
                  observationKey
                  authorName
                  reactionCount
                  capturedAt
                  rawText { raw }
                }
              }
            }
          }
        }
      `,
      variables: { after },
    });
    feeds.push(...result.sourceFeedsConnection.edges.map((edge) => edge.node));
    after = result.sourceFeedsConnection.pageInfo.hasNextPage
      ? result.sourceFeedsConnection.pageInfo.endCursor
      : undefined;
  } while (after);
  return feeds;
}

async function readDossiers() {
  const result = await graphQlRequest({
    endpoint,
    token,
    query: `
      query DraftModDossiers {
        modDossiers(stage: DRAFT, first: 100) {
          dossierKey
          title
          author
          primarySourceFeed { sourceKey }
          officialUpdateFeed { sourceKey }
          goldCampaign { campaignId title }
        }
      }
    `,
  });
  return result.modDossiers;
}

function toPromotion(feed, matcher) {
  const observation = feed.observations.find((candidate) => plainText(candidate.rawText?.raw).includes(forumCardMarker));
  if (!observation) return null;
  const parsed = parseForumCard(observation.rawText?.raw);
  const match = matcher(feed, parsed);
  return {
    feed,
    observation,
    parsed,
    match,
    dossierKey: match?.dossierKey ?? `discord-forum-${feed.channelId}`,
  };
}

function createInput(item) {
  const status = statusFromTags(item.parsed.tags);
  const branch = branchFromTags(item.parsed.tags);
  return stripUndefined({
    dossierKey: item.dossierKey,
    title: item.feed.title,
    author: item.observation.authorName ?? item.parsed.author,
    branch,
    sourceEvidence: "CONTENT_VERIFIED",
    authorStatus: status,
    ccmCompatibility: "UNKNOWN",
    reviewStatus: "DRAFT",
    primarySourceFeed: { connect: { sourceKey: item.feed.sourceKey } },
    originDescriptionObservation: { connect: { observationKey: item.observation.observationKey } },
    popularitySnapshot: item.observation.reactionCount,
    popularityObservedAt: item.observation.reactionCount === null || item.observation.reactionCount === undefined
      ? undefined
      : item.observation.capturedAt,
    sourceNotes: richTextFromParagraphs([
      "Created automatically from a Discord forum-card preview in Bronze.",
      "Read the linked Bronze observation and the full Discord thread before drafting public copy, selecting a latest relevant update, or adding package/version data.",
      "No Gold campaign or production catalog entry has been created from this record.",
    ]),
  });
}

function updateInput(item) {
  // Do not overwrite human work.  This deliberately excludes public copy,
  // status, author, version, download, Gold links, and latest-update selection.
  return stripUndefined({
    primarySourceFeed: { connect: { sourceKey: item.feed.sourceKey } },
    originDescriptionObservation: { connect: { observationKey: item.observation.observationKey } },
    popularitySnapshot: item.observation.reactionCount,
    popularityObservedAt: item.observation.reactionCount === null || item.observation.reactionCount === undefined
      ? undefined
      : item.observation.capturedAt,
    sourceEvidence: "CONTENT_VERIFIED",
  });
}

function buildMatcher(dossiers, aliasesByDossierKey) {
  // `discord-forum-*` rows are generated candidates, not a canonical identity
  // registry.  Letting later cards fuzzy-match them would silently merge two
  // unrelated projects on a re-run.  Only the pre-existing dossier set may
  // receive a conservative fuzzy match; generated candidates always retain one
  // dossier per forum thread.
  const canonicalDossiers = dossiers.filter((dossier) => !dossier.dossierKey.startsWith("discord-forum-"));
  const byFeed = new Map();
  for (const dossier of canonicalDossiers) {
    for (const feed of [dossier.primarySourceFeed, dossier.officialUpdateFeed]) {
      if (feed?.sourceKey) byFeed.set(feed.sourceKey, dossier);
    }
  }

  return (feed, parsed) => {
    const linked = byFeed.get(feed.sourceKey);
    if (linked) return { dossierKey: linked.dossierKey, reason: "existing source feed" };

    const title = normalizeTitle(feed.title);
    const branch = branchFromTags(parsed.tags);
    const ranked = canonicalDossiers
      .filter((dossier) => dossier.goldCampaign)
      .map((dossier) => {
      const aliasList = aliasesByDossierKey.get(dossier.dossierKey) ?? [normalizeTitle(dossier.title)];
      const score = Math.max(...aliasList.map((alias) => titleSimilarity(title, alias)));
      const authorMatch = namesComparable(parsed.author, dossier.author);
      const goldBranch = branchFromCampaignId(dossier.goldCampaign?.campaignId);
      return { dossier, score: authorMatch ? Math.min(1, score + 0.04) : score, goldBranch };
    }).filter((candidate) => !branch || !candidate.goldBranch || candidate.goldBranch === branch)
      .sort((left, right) => right.score - left.score);

    const best = ranked[0];
    const runnerUp = ranked[1];
    // A match must be very strong and distinctly better than the runner-up.
    // Anything less is intentionally left as a separate DRAFT candidate for a
    // human to merge; a false merge is much costlier than a duplicate dossier.
    if (best && best.score >= 0.92 && (!runnerUp || best.score - runnerUp.score >= 0.12)) {
      return { dossierKey: best.dossier.dossierKey, reason: "conservative title match" };
    }
    return null;
  };
}

function aliasesFromCatalog(catalog) {
  const aliases = new Map();
  for (const campaign of catalog.campaigns) {
    aliases.set(campaign.id, [normalizeTitle(campaign.title), normalizeTitle(campaign.id)]);
  }
  return aliases;
}

function parseForumCard(raw) {
  const lines = plainText(raw).split("\n").map((line) => line.trim());
  const value = (name) => lines.find((line) => line.startsWith(`${name}:`))?.slice(name.length + 1).trim();
  return {
    author: value("Author"),
    tags: (value("Tags") ?? "").split(",").map((tag) => tag.trim()).filter(Boolean),
  };
}

function branchFromTags(tags) {
  const branches = new Set();
  for (const tag of tags) {
    if (tag === "WoL") branches.add("WINGS_OF_LIBERTY");
    if (tag === "HotS") branches.add("HEART_OF_THE_SWARM");
    if (tag === "LotV") branches.add("LEGACY_OF_THE_VOID");
    if (tag === "N:CO") branches.add("NOVA_COVERT_OPS");
  }
  return branches.size === 1 ? [...branches][0] : undefined;
}

function branchFromCampaignId(campaignId) {
  // Gold campaign IDs do not encode their branch consistently.  Their title
  // aliases still participate in matching; branch only narrows clear cases.
  if (/^(coop-ai|junker-edition|lings-of-wiberty|mindhawk-s-gauntlet|moebius-pack|nightmare-difficulty-wings-of-liberty|raynor-has-gone-rogue-like|real-scale-wol|wol-|wings-of-liberty)/.test(campaignId ?? "")) return "WINGS_OF_LIBERTY";
  if (/^(abathur-s-whimsy|hots-|heart-of-the-swarm|kerrigan-has-gone-rogue-like|nightmare-difficulty$|real-scale-heart|the-swarm-reborn|violet-s-hots|f-yuri)/.test(campaignId ?? "")) return "HEART_OF_THE_SWARM";
  if (/^(aeon|artanis-has-gone-zeratul-like|fight-with-ally|legacy-|lotv-|nightmare-difficulty-legacy|real-scale-lotv)/.test(campaignId ?? "")) return "LEGACY_OF_THE_VOID";
  if (/^(avon|nco-|nova-|mod-ccm)/.test(campaignId ?? "")) return "NOVA_COVERT_OPS";
  return undefined;
}

function statusFromTags(tags) {
  if (tags.includes("Complete")) return "COMPLETE";
  if (tags.includes("In Progress") || tags.includes("Initial Development") || tags.includes("Near Complete")) return "IN_PROGRESS";
  return "UNKNOWN";
}

function titleSimilarity(left, right) {
  if (!left || !right) return 0;
  if (left === right) return 1;
  const leftTokens = new Set(left.split(" "));
  const rightTokens = new Set(right.split(" "));
  const intersection = [...leftTokens].filter((token) => rightTokens.has(token)).length;
  const precision = intersection / leftTokens.size;
  const recall = intersection / rightTokens.size;
  return (2 * precision * recall) / (precision + recall) || 0;
}

function normalizeTitle(value) {
  return `${value ?? ""}`
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/wings\s+of\s+liberty/g, " wol ")
    .replace(/heart\s+of\s+the\s+swarm/g, " hots ")
    .replace(/legacy\s+of\s+the\s+void/g, " lotv ")
    .replace(/nova\s+covert\s+ops/g, " nco ")
    .replace(/\b(?:v(?:ersion)?\s*)?\d+(?:\.\d+)+(?:\s*[a-z0-9.-]+)?\b/g, " ")
    .replace(/\b(?:complete|completed|official|release|version|translated|mod|campaign|edition|the)\b/g, " ")
    .replace(/[^a-z0-9]+/g, " ")
    .trim()
    .replace(/\s+/g, " ");
}

function namesComparable(left, right) {
  const normalizedLeft = normalizeTitle(left);
  const normalizedRight = normalizeTitle(right);
  return normalizedLeft.length >= 3 && normalizedRight.length >= 3 && (normalizedLeft === normalizedRight || normalizedLeft.includes(normalizedRight) || normalizedRight.includes(normalizedLeft));
}

function plainText(value) {
  if (!value || typeof value !== "object") return "";
  const text = typeof value.text === "string" ? [value.text] : [];
  const children = Array.isArray(value.children) ? value.children.flatMap((child) => plainText(child)) : [];
  return [...text, ...children].join("\n");
}

function richTextFromParagraphs(paragraphs) {
  return { children: paragraphs.map((text) => ({ type: "paragraph", children: [{ text }] })) };
}

function stripUndefined(value) {
  return Object.fromEntries(Object.entries(value).filter(([, item]) => item !== undefined));
}

function numberFlag(flag, fallback) {
  const values = flagValues(flag);
  assert(values.length <= 1, `${flag} may be specified once.`);
  if (!values.length) return fallback;
  const value = Number(values[0]);
  assert(Number.isSafeInteger(value), `${flag} must be an integer.`);
  return value;
}

async function readJson(path) {
  return JSON.parse(await readFile(path, "utf8"));
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
