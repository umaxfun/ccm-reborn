import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { flagValues, graphQlRequest, hasFlag, requireEnvironment } from "./common.mjs";

// Imports the current local-RPC message window as Bronze evidence.  These are
// deliberately not promoted to "latest relevant update": a reviewer or agent
// must choose that semantic role after reading the messages.
const apply = hasFlag("--apply");
const inputPath = resolve(process.cwd(), flagValues("--input")[0] ?? "work/discord-descriptions.json");
const input = JSON.parse(await readFile(inputPath, "utf8"));

assert(input.format === 1 && Array.isArray(input.threads), "Discord RPC export must be format 1.");
const entries = input.threads.flatMap((thread) => (thread.messages ?? []).map((message) => makeEntry(thread, message)));
assert(entries.length > 0, "Discord RPC export contains no messages.");
assert(new Set(entries.map((entry) => entry.observationKey)).size === entries.length, "Discord RPC export contains duplicate observations.");

if (!apply) {
  console.log(`Dry run only. Ready to import ${entries.length} sidebar-message Bronze snapshots from ${input.threads.length} channels.`);
  console.log(`${input.unavailableSources?.length ?? 0} source channels were unavailable to local RPC and remain feeds without message snapshots.`);
  console.log("No semantic latest-update relation, Silver review field, Gold record, production JSON, or publication will be changed.");
  process.exit(0);
}

const endpoint = requireEnvironment("HYGRAPH_CONTENT_ENDPOINT");
const token = requireEnvironment("HYGRAPH_TOKEN");
let lastRequestAt = 0;
const feeds = uniqueBySourceKey(input.threads.map(makeFeed));
for (const feed of feeds) {
  await upsertFeed(feed);
}
const existingKeys = await readExistingKeys(entries.map((entry) => entry.observationKey));
const capturedAt = new Date().toISOString();

const createObservation = `
  mutation CreateSourceObservation($data: SourceObservationCreateInput!) {
    createSourceObservation(data: $data) { observationKey }
  }
`;

let created = 0;
let reused = 0;
for (const entry of entries) {
  if (existingKeys.has(entry.observationKey)) {
    reused += 1;
    continue;
  }
  await contentRequest(createObservation, {
    data: {
      observationKey: entry.observationKey,
      messageId: entry.messageId,
      messageUrl: entry.messageUrl,
      rawText: richTextFromRaw(entry.rawText),
      authorName: entry.authorName,
      messageCreatedAt: entry.messageCreatedAt,
      messageEditedAt: entry.messageEditedAt,
      capturedAt,
      contentFingerprint: entry.contentFingerprint,
      reactionCount: entry.reactionCount,
      reactionObservedAt: entry.reactionCount === undefined ? undefined : capturedAt,
      reactions: entry.reactions,
      sourceFeed: { connect: { sourceKey: entry.sourceKey } },
    },
  });
  created += 1;
}

console.log(`Upserted ${feeds.length} Bronze feeds and imported sidebar-message snapshots: ${created} created, ${reused} unchanged.`);

async function upsertFeed(feed) {
  await contentRequest(`
    mutation UpsertDiscordRpcFeed($where: SourceFeedWhereUniqueInput!, $upsert: SourceFeedUpsertInput!) {
      upsertSourceFeed(where: $where, upsert: $upsert) { sourceKey }
    }
  `, {
    where: { sourceKey: feed.sourceKey },
    upsert: {
      create: feed,
      update: {
        title: feed.title,
        sourceUrl: feed.sourceUrl,
        isOfficial: true,
        isActive: true,
      },
    },
  });
}

async function readExistingKeys(keys) {
  const found = new Set();
  for (const group of chunk(keys, 50)) {
    const result = await contentRequest(`
      query ExistingSourceObservations($keys: [String!]) {
        sourceObservations(stage: DRAFT, first: 100, where: { observationKey_in: $keys }) { observationKey }
      }
    `, { keys: group });
    for (const observation of result.sourceObservations) found.add(observation.observationKey);
  }
  return found;
}

async function contentRequest(query, variables) {
  const waitMs = Math.max(0, 260 - (Date.now() - lastRequestAt));
  if (waitMs) await new Promise((resolvePromise) => setTimeout(resolvePromise, waitMs));
  lastRequestAt = Date.now();
  return graphQlRequest({ endpoint, token, query, variables });
}

function makeEntry(thread, message) {
  assert(typeof thread?.id === "string" && typeof thread?.guildId === "string", "Discord RPC thread lacks an ID or guild ID.");
  assert(typeof message?.id === "string", `Discord RPC message in ${thread.id} lacks an ID.`);
  const attachmentLines = (message.attachments ?? []).map((attachment) => `Attachment: ${attachment.filename ?? "unnamed"}${attachment.url ? ` — ${attachment.url}` : ""}`);
  const content = message.content?.trim() || "[No text content]";
  const rawText = [
    "Discord local-RPC recent-message snapshot. It is raw Bronze evidence, not a selected release update.",
    `Channel: ${thread.name ?? thread.id}`,
    `Author: ${message.author?.globalName ?? message.author?.username ?? "Unknown"}`,
    `Created: ${message.createdAt ?? "Unknown"}`,
    message.updatedAt ? `Edited: ${message.updatedAt}` : "",
    "Message:",
    content,
    ...attachmentLines,
  ].filter(Boolean).join("\n");
  const contentFingerprint = sha256(rawText);
  const sourceKey = `discord:${thread.guildId}:${thread.id}`;
  return {
    observationKey: `${sourceKey}:${message.id}:${contentFingerprint.slice(0, 16)}`,
    sourceKey,
    messageId: message.id,
    messageUrl: message.url ?? `https://discord.com/channels/${thread.guildId}/${thread.id}/${message.id}`,
    rawText,
    authorName: message.author?.globalName ?? message.author?.username,
    messageCreatedAt: message.createdAt,
    messageEditedAt: message.updatedAt,
    contentFingerprint,
    reactionCount: Number.isSafeInteger(message.reactionCount) ? message.reactionCount : undefined,
    reactions: message.reactionDataAvailable ? message.reactions : undefined,
  };
}

function makeFeed(thread) {
  assert(typeof thread?.id === "string" && typeof thread?.guildId === "string", "Discord RPC thread lacks an ID or guild ID.");
  return {
    sourceKey: `discord:${thread.guildId}:${thread.id}`,
    title: thread.name ?? thread.id,
    sourceUrl: `https://discord.com/channels/${thread.guildId}/${thread.id}`,
    guildId: thread.guildId,
    channelId: thread.id,
    kind: "DISCORD_DEDICATED_CHANNEL",
    cadence: "MANUAL",
    isOfficial: true,
    isActive: true,
  };
}

function richTextFromRaw(rawText) {
  return {
    children: rawText.split(/\n{2,}/).map((text) => ({ type: "paragraph", children: [{ text }] })),
  };
}

function sha256(value) {
  return createHash("sha256").update(value, "utf8").digest("hex");
}

function chunk(items, size) {
  return Array.from({ length: Math.ceil(items.length / size) }, (_unused, index) => items.slice(index * size, index * size + size));
}

function uniqueBySourceKey(feeds) {
  return [...new Map(feeds.map((feed) => [feed.sourceKey, feed])).values()];
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
