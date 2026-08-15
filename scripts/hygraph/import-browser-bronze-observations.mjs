import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { flagValues, graphQlRequest, hasFlag, requireEnvironment } from "./common.mjs";

// Persists immutable snapshots read from the signed-in Discord UI. The input
// may introduce a source feed that has no dossier yet, but it never mutates a
// Silver dossier, Gold Campaign, generated catalog, or publication state.
const apply = hasFlag("--apply");
const inputPath = resolve(process.cwd(), requiredSingleFlag("--input"));
const input = JSON.parse(await readFile(inputPath, "utf8"));
const selectedMessageIds = new Set(flagValues("--message-id"));

assert(input.format === 1 && Array.isArray(input.feeds) && Array.isArray(input.observations), "Browser Bronze input must have format 1, feeds, and observations.");
const feeds = new Map();
for (const feed of input.feeds) {
  for (const field of ["sourceKey", "title", "sourceUrl", "guildId", "channelId"]) assert(typeof feed?.[field] === "string" && feed[field].trim(), `Feed has no ${field}.`);
  assert(!feeds.has(feed.sourceKey), `Duplicate feed: ${feed.sourceKey}`);
  feeds.set(feed.sourceKey, feed);
}
const observations = input.observations
  .filter((entry) => !selectedMessageIds.size || selectedMessageIds.has(entry.messageId))
  .map(validateObservation);
assert(observations.length > 0, selectedMessageIds.size ? "No observations match the requested --message-id values." : "Input contains no observations.");
assert(new Set(observations.map((entry) => entry.observationKey)).size === observations.length, "Input has duplicate Bronze observations.");

if (!apply) {
  console.log(`Dry run only. Ready to upsert ${feeds.size} Browser-read Bronze feeds and create ${observations.length} immutable message snapshots.`);
  console.log("No Silver dossier, Gold Campaign, production JSON, or publication will be changed.");
  process.exit(0);
}

const endpoint = requireEnvironment("HYGRAPH_CONTENT_ENDPOINT");
const token = requireEnvironment("HYGRAPH_TOKEN");
const capturedAt = new Date().toISOString();
let lastRequestAt = 0;
for (const feed of feeds.values()) {
  const { sourceKey, ...data } = feed;
  await request(`
    mutation UpsertBrowserSourceFeed($where: SourceFeedWhereUniqueInput!, $upsert: SourceFeedUpsertInput!) {
      upsertSourceFeed(where: $where, upsert: $upsert) { sourceKey }
    }
  `, {
    where: { sourceKey },
    upsert: {
      create: { ...data, sourceKey, kind: feed.kind ?? "DISCORD_DEDICATED_CHANNEL", cadence: feed.cadence ?? "QUARTERLY", isOfficial: true, isActive: true },
      update: { title: feed.title, sourceUrl: feed.sourceUrl, isOfficial: true, isActive: true },
    },
  });
}

const existingKeys = await readExistingKeys(observations.map((entry) => entry.observationKey));
let created = 0;
for (const observation of observations) {
  if (existingKeys.has(observation.observationKey)) continue;
  await request(`
    mutation CreateBrowserSourceObservation($data: SourceObservationCreateInput!) {
      createSourceObservation(data: $data) { observationKey }
    }
  `, {
    data: {
      observationKey: observation.observationKey,
      messageId: observation.messageId,
      messageUrl: observation.messageUrl,
      rawText: richTextFromRaw(observation.rawText),
      authorName: observation.authorName,
      messageCreatedAt: observation.messageCreatedAt,
      capturedAt,
      contentFingerprint: observation.contentFingerprint,
      reactionCount: observation.reactionCount,
      reactionObservedAt: observation.reactionCount === undefined ? undefined : capturedAt,
      sourceFeed: { connect: { sourceKey: observation.sourceKey } },
    },
  });
  created += 1;
}
console.log(`Imported ${created} Browser-read Bronze observations; ${observations.length - created} were unchanged. No Silver or Gold content changed.`);

function validateObservation(entry) {
  for (const field of ["sourceKey", "messageId", "messageUrl", "rawText"]) assert(typeof entry?.[field] === "string" && entry[field].trim(), `Observation has no ${field}.`);
  assert(feeds.has(entry.sourceKey), `Observation refers to unknown feed: ${entry.sourceKey}`);
  assert(entry.messageUrl === `https://discord.com/channels/${feeds.get(entry.sourceKey).guildId}/${feeds.get(entry.sourceKey).channelId}/${entry.messageId}`, `Unexpected Discord message URL: ${entry.messageUrl}`);
  const contentFingerprint = sha256(entry.rawText);
  return {
    ...entry,
    authorName: entry.authorName?.trim() || undefined,
    messageCreatedAt: entry.messageCreatedAt ?? discordSnowflakeTimestamp(entry.messageId),
    observationKey: `${entry.sourceKey}:${entry.messageId}:${contentFingerprint.slice(0, 16)}`,
    contentFingerprint,
  };
}

async function readExistingKeys(keys) {
  const found = new Set();
  for (const group of chunk(keys, 50)) {
    const result = await request(`
      query ExistingBrowserObservations($keys: [String!]) {
        sourceObservations(stage: DRAFT, first: 100, where: { observationKey_in: $keys }) { observationKey }
      }
    `, { keys: group });
    for (const item of result.sourceObservations) found.add(item.observationKey);
  }
  return found;
}

async function request(query, variables) {
  const delay = Math.max(0, 260 - (Date.now() - lastRequestAt));
  if (delay) await new Promise((resolvePromise) => setTimeout(resolvePromise, delay));
  lastRequestAt = Date.now();
  return graphQlRequest({ endpoint, token, query, variables });
}

function richTextFromRaw(rawText) {
  return { children: rawText.split(/\n{2,}/).filter(Boolean).map((text) => ({ type: "paragraph", children: [{ text }] })) };
}

function discordSnowflakeTimestamp(messageId) {
  return new Date(Number((BigInt(messageId) >> 22n) + 1420070400000n)).toISOString();
}

function sha256(value) {
  return createHash("sha256").update(value, "utf8").digest("hex");
}

function requiredSingleFlag(flag) {
  const values = flagValues(flag);
  assert(values.length <= 1, `${flag} may be specified once.`);
  assert(values.length === 1, `${flag} is required; pass an explicitly reviewed Browser Bronze input file.`);
  return values[0];
}

function chunk(items, size) {
  return Array.from({ length: Math.ceil(items.length / size) }, (_unused, index) => items.slice(index * size, index * size + size));
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
