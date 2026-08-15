import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { flagValues, graphQlRequest, hasFlag, requireEnvironment } from "./common.mjs";

// Moves a reviewed agent-writing batch from DRAFT to IN_REVIEW. This remains a
// human gate: there is deliberately no `--publish` mode and no Gold mutation.
const apply = hasFlag("--apply");
const inputPath = inputFilePath();
const input = JSON.parse(await readFile(inputPath, "utf8"));

assert(input.format === 1 && Array.isArray(input.dossiers) && input.dossiers.length > 0, "Agent editorial batch must be format 1 with dossiers.");
assert(new Set(input.dossiers.map((item) => item.dossierKey)).size === input.dossiers.length, "Agent editorial batch contains duplicate dossier keys.");
for (const item of input.dossiers) validate(item);

if (!apply) {
  console.log(`Dry run only. ${input.dossiers.length} source-backed Silver drafts are ready for human review.`);
  console.log("No Gold campaign, production JSON, or publication will be changed.");
  process.exit(0);
}

const endpoint = requireEnvironment("HYGRAPH_CONTENT_ENDPOINT");
const token = requireEnvironment("HYGRAPH_TOKEN");
const known = await readDraftDossiers(input.dossiers.map((item) => item.dossierKey));
for (const item of input.dossiers) assert(known.has(item.dossierKey), `Unknown draft dossier: ${item.dossierKey}`);

let lastMutationStartedAt = 0;
for (const item of input.dossiers) {
  await contentMutation(`
    mutation ApplyAgentEditorial($where: ModDossierWhereUniqueInput!, $data: ModDossierUpdateInput!) {
      updateModDossier(where: $where, data: $data) { dossierKey reviewStatus }
    }
  `, {
    where: { dossierKey: item.dossierKey },
    data: {
      shortDescription: item.shortDescription,
      editorialBody: richTextFromParagraphs(item.editorialBody),
      tags: item.tags,
      chooseThisIf: item.chooseThisIf,
      avoidThisIf: item.avoidThisIf,
      latestKnownVersion: item.latestKnownVersion,
      downloadUrl: item.downloadUrl,
      reviewStatus: "IN_REVIEW",
    },
  });
  console.log(`Submitted for human review: ${item.dossierKey}`);
}
console.log(`Submitted ${input.dossiers.length} Silver dossiers for human review. Nothing was published or promoted to Gold.`);

async function readDraftDossiers(keys) {
  const result = await graphQlRequest({
    endpoint,
    token,
    query: `
      query DraftDossiers($keys: [String!]) {
        modDossiers(stage: DRAFT, first: 100, where: { dossierKey_in: $keys }) { dossierKey }
      }
    `,
    variables: { keys },
  });
  return new Set(result.modDossiers.map((item) => item.dossierKey));
}

async function contentMutation(query, variables) {
  const waitMs = Math.max(0, 260 - (Date.now() - lastMutationStartedAt));
  if (waitMs) await new Promise((resolvePromise) => setTimeout(resolvePromise, waitMs));
  lastMutationStartedAt = Date.now();
  return graphQlRequest({ endpoint, token, query, variables });
}

function richTextFromParagraphs(paragraphs) {
  return { children: paragraphs.map((text) => ({ type: "paragraph", children: [{ text }] })) };
}

function validate(item) {
  const prefix = `Agent editorial ${item?.dossierKey ?? "<unknown>"}`;
  assert(typeof item?.dossierKey === "string" && item.dossierKey, `${prefix} must target a dossier.`);
  assert(typeof item.shortDescription === "string" && item.shortDescription.length >= 70 && item.shortDescription.length <= 350, `${prefix} needs a 70–350 character short description.`);
  assert(Array.isArray(item.editorialBody) && item.editorialBody.length >= 2 && item.editorialBody.every((text) => typeof text === "string" && text.trim()), `${prefix} needs at least two editorial paragraphs.`);
  assert(Array.isArray(item.tags) && item.tags.length >= 3, `${prefix} needs at least three tags.`);
  for (const field of ["chooseThisIf", "avoidThisIf"]) assert(Array.isArray(item[field]) && item[field].length >= 2, `${prefix} needs two ${field} entries.`);
  for (const optional of ["latestKnownVersion", "downloadUrl"]) assert(item[optional] === undefined || typeof item[optional] === "string", `${prefix} has invalid ${optional}.`);
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function inputFilePath() {
  const values = flagValues("--input");
  assert(values.length <= 1, "--input may be specified once.");
  assert(values.length === 1, "--input is required; pass a reviewed agent editorial batch.");
  return resolve(process.cwd(), values[0]);
}
