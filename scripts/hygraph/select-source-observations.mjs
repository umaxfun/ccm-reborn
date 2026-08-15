import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { flagValues, graphQlRequest, hasFlag, requireEnvironment } from "./common.mjs";

// Makes the human/agent decision between several Bronze messages explicit. A
// channel's introductory post, download post, and patch notes are distinct
// observations; the script intentionally never ranks them by date or length.
// The selected messages are materialized in one Silver sourceContext field so
// an editor can review the complete relevant evidence in one place.
const apply = hasFlag("--apply");
const input = requiredSingleFlag("--input");
const endpoint = requireEnvironment("HYGRAPH_CONTENT_ENDPOINT");
const token = requireEnvironment("HYGRAPH_TOKEN");
const document = JSON.parse(await readFile(resolve(input), "utf8"));
const selections = validateDocument(document);

for (const selection of selections) {
  const dossier = await readDossier(selection.dossierKey);
  const observations = dossier.primarySourceFeed?.observations ?? [];
  const byMessageId = groupByMessageId(observations);
  const origin = mostCompleteObservation(byMessageId.get(selection.originMessageId));
  const latest = mostCompleteObservation(byMessageId.get(selection.latestUpdateMessageId));
  const context = selection.contextMessageIds.map((messageId) => mostCompleteObservation(byMessageId.get(messageId)));
  if (!origin || !latest || context.some((observation) => !observation)) {
    const known = [...byMessageId.keys()].join(", ") || "none";
    throw new Error(`${selection.dossierKey}: selected message is not a Bronze observation of its primary source feed. Known IDs: ${known}`);
  }
  const data = stripUndefined({
    originDescriptionObservation: { connect: { observationKey: origin.observationKey } },
    latestUpdateObservation: { connect: { observationKey: latest.observationKey } },
    sourceContext: richTextFromObservations(context),
    latestKnownVersion: selection.latestKnownVersion,
    downloadUrl: selection.downloadUrl,
    sourceEvidence: selection.sourceEvidence,
  });
  console.log(`${selection.dossierKey}\n  origin: ${origin.messageUrl}\n  latest: ${latest.messageUrl}\n  context: ${context.map((observation) => observation.messageUrl).join(" | ")}`);
  if (apply) {
    await graphQlRequest({
      endpoint,
      token,
      query: `
        mutation SelectSourceObservations($where: ModDossierWhereUniqueInput!, $data: ModDossierUpdateInput!) {
          updateModDossier(where: $where, data: $data) { dossierKey }
        }
      `,
      variables: { where: { dossierKey: selection.dossierKey }, data },
    });
  }
}

console.log(`${apply ? "Applied" : "Validated"} ${selections.length} explicit Bronze-to-Silver source selections. No Gold record or publication was changed.`);

async function readDossier(dossierKey) {
  const result = await graphQlRequest({
    endpoint,
    token,
    query: `
      query SourceSelectionDossier($dossierKey: String!) {
        modDossier(where: { dossierKey: $dossierKey }, stage: DRAFT) {
          dossierKey
          primarySourceFeed {
            sourceKey
            observations(first: 100) {
              observationKey
              messageId
              messageUrl
              authorName
              messageCreatedAt
              rawText { raw }
            }
          }
        }
      }
    `,
    variables: { dossierKey },
  });
  if (!result.modDossier) throw new Error(`No draft ModDossier found for ${dossierKey}.`);
  if (!result.modDossier.primarySourceFeed) throw new Error(`${dossierKey} has no primarySourceFeed.`);
  return result.modDossier;
}

function validateDocument(document) {
  if (!document || !Array.isArray(document.selections)) throw new Error("Selection input must contain a selections array.");
  const keys = new Set();
  for (const selection of document.selections) {
    for (const field of ["dossierKey", "originMessageId", "latestUpdateMessageId"]) {
      if (typeof selection?.[field] !== "string" || !selection[field].trim()) throw new Error(`Selection ${field} must be a non-empty string.`);
    }
    if (selection.contextMessageIds !== undefined && (!Array.isArray(selection.contextMessageIds) || !selection.contextMessageIds.length || selection.contextMessageIds.some((id) => typeof id !== "string" || !id.trim()))) {
      throw new Error(`${selection.dossierKey}: contextMessageIds must be a non-empty array of message IDs.`);
    }
    selection.contextMessageIds = [...new Set(selection.contextMessageIds ?? [selection.originMessageId, selection.latestUpdateMessageId])];
    if (keys.has(selection.dossierKey)) throw new Error(`Duplicate dossierKey in selection input: ${selection.dossierKey}`);
    keys.add(selection.dossierKey);
  }
  return document.selections;
}

function requiredSingleFlag(flag) {
  const values = flagValues(flag);
  if (values.length !== 1) throw new Error(`${flag} is required.`);
  return values[0];
}

function stripUndefined(value) {
  return Object.fromEntries(Object.entries(value).filter(([, item]) => item !== undefined));
}

function richTextFromObservations(observations) {
  const children = [];
  for (const observation of observations) {
    children.push({ type: "paragraph", children: [{ text: `Discord source message — ${observation.authorName ?? "unknown author"}${observation.messageCreatedAt ? `, ${observation.messageCreatedAt}` : ""}` }] });
    children.push({ type: "paragraph", children: [{ text: observation.messageUrl }] });
    for (const paragraph of plainText(observation.rawText?.raw).split(/\n{2,}/).map((text) => text.trim()).filter(Boolean)) {
      children.push({ type: "paragraph", children: [{ text: paragraph }] });
    }
  }
  return { children };
}

function plainText(value) {
  if (!value || typeof value !== "object") return "";
  const text = typeof value.text === "string" ? [value.text] : [];
  const children = Array.isArray(value.children) ? value.children.flatMap((child) => plainText(child)) : [];
  return [...text, ...children].join("\n");
}

function groupByMessageId(observations) {
  const grouped = new Map();
  for (const observation of observations) {
    const entries = grouped.get(observation.messageId) ?? [];
    entries.push(observation);
    grouped.set(observation.messageId, entries);
  }
  return grouped;
}

function mostCompleteObservation(observations) {
  if (!observations?.length) return undefined;
  // The same Discord post can be captured by a forum-card preview and by the
  // signed-in client. Select the longest immutable capture so the reviewer sees
  // the most complete version rather than a truncated preview.
  return [...observations].sort((left, right) => plainText(right.rawText?.raw).length - plainText(left.rawText?.raw).length)[0];
}
