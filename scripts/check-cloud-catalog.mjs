import { createHash } from "node:crypto";

const catalogUrl = process.env.CCM_CATALOG_URL ?? "https://files.ccm-reborn.mikilabs.io/catalog.json";
const verifyIndex = process.argv.indexOf("--verify");
const verifyId = verifyIndex === -1 ? "" : process.argv[verifyIndex + 1] ?? "";

function fail(message) {
  throw new Error(message);
}

function isSha256(value) {
  return typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
}

async function fetchRequired(url, options = {}) {
  const response = await fetch(url, { redirect: "error", cache: "no-store", ...options });
  if (!response.ok) fail(`${url} returned HTTP ${response.status}.`);
  return response;
}

async function verifyArchive(campaign) {
  const response = await fetchRequired(campaign.package.url);
  const reader = response.body?.getReader();
  if (!reader) fail(`Could not read ${campaign.package.url}.`);
  const hash = createHash("sha256");
  let bytes = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    bytes += value.byteLength;
    hash.update(value);
  }
  if (bytes !== campaign.package.size || hash.digest("hex") !== campaign.package.sha256) {
    fail(`${campaign.id} does not match the catalog checksum or size.`);
  }
}

const catalogResponse = await fetchRequired(catalogUrl);
const catalog = await catalogResponse.json();
if (catalog.format !== 1 || !Array.isArray(catalog.campaigns) || !catalog.campaigns.length) {
  fail("Cloud catalog is not a non-empty format-1 catalog.");
}
const ids = new Set();
for (const campaign of catalog.campaigns) {
  if (!campaign?.id || ids.has(campaign.id)) fail("Cloud catalog contains an invalid or duplicate campaign ID.");
  ids.add(campaign.id);
  const packageUrl = campaign.package?.url;
  if (typeof packageUrl !== "string" || !packageUrl.startsWith("https://") || !isSha256(campaign.package?.sha256)) {
    fail(`${campaign.id} has an invalid public package declaration.`);
  }
  const size = campaign.package?.size;
  if (!Number.isSafeInteger(size) || size <= 0) fail(`${campaign.id} has an invalid package size.`);
  const response = await fetchRequired(packageUrl, { method: "HEAD" });
  if (Number(response.headers.get("content-length")) !== size) fail(`${campaign.id} has an unexpected public package size.`);
}
if (verifyId) {
  const campaign = catalog.campaigns.find((item) => item.id === verifyId);
  if (!campaign) fail(`No campaign with ID ${verifyId} exists in the cloud catalog.`);
  await verifyArchive(campaign);
  console.log(`Verified ${verifyId}: checksum and size match.`);
}
console.log(`Cloud catalog is healthy: ${catalog.campaigns.length} campaign(s) reachable.`);
