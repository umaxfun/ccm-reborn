import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { compareCatalogs, makeProductionCatalog } from "./catalog-format.mjs";
import { flagValues, graphQlRequest, hasFlag, requireEnvironment } from "./common.mjs";

const output = resolve(process.cwd(), process.env.CMS_CATALOG_OUTPUT ?? "work/hygraph-catalog.json");
const baselinePath = resolve(process.cwd(), "catalog/catalog.json");
const upload = hasFlag("--upload");
const allowedRemovals = new Set(flagValues("--allow-remove"));
const endpoint = requireEnvironment("HYGRAPH_HP_CONTENT_ENDPOINT");
const token = requireEnvironment("HYGRAPH_TOKEN");
const publicBaseUrl = (process.env.CLOUDFLARE_R2_PUBLIC_BASE_URL ?? "https://files.ccm-reborn.mikilabs.io").replace(/\/$/, "");

const query = `
  query PublishedCampaigns {
    campaigns(stage: PUBLISHED, first: 1000) {
      campaignId
      catalogOrder
      title
      author
      shortDescription
      tags
      branch
      currentRelease {
        releaseKey
        version
        packageUrl
        packageSha256
        packageSize
      }
    }
  }
`;

const data = await graphQlRequest({ endpoint, token, query });
const localBaseline = JSON.parse(await readFile(baselinePath, "utf8"));

async function fetchRemoteText(url) {
  const probe = new URL(url);
  probe.searchParams.set("ccm-cms-probe", `${Date.now()}-${Math.random().toString(16).slice(2)}`);
  const response = await fetch(probe, { cache: "no-store", redirect: "error" });
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`Could not fetch ${url}: HTTP ${response.status}.`);
  return response.text();
}

const remoteCatalogText = upload ? await fetchRemoteText(`${publicBaseUrl}/catalog.json`) : null;
const baseline = remoteCatalogText === null ? localBaseline : JSON.parse(remoteCatalogText);
const catalog = makeProductionCatalog(data.campaigns, baseline);
const changes = compareCatalogs(baseline, catalog, allowedRemovals);
await mkdir(dirname(output), { recursive: true });
const outputText = `${JSON.stringify(catalog, null, 2)}\n`;
await writeFile(output, outputText, "utf8");
console.log(`Generated ${catalog.campaigns.length} campaigns at ${output}.`);
console.log(`Catalog diff: ${changes.added.length} added, ${changes.removed.length} removed.`);

if (!upload) {
  console.log("Validation passed. Use --upload to publish history and catalog.json to R2.");
  process.exit(0);
}

const bucket = process.env.CLOUDFLARE_R2_BUCKET ?? "ccm-reborn";
const timestampKey = catalog.updatedAt.replace(/[:.]/g, "-");
const historyKey = `catalog-history/${timestampKey}.json`;

function runWrangler(args) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn("wrangler", args, { cwd: process.cwd(), stdio: "inherit" });
    child.on("error", (error) => reject(new Error(`Could not start Wrangler: ${error.message}`)));
    child.on("exit", (code) => code === 0 ? resolvePromise() : reject(new Error(`Wrangler exited with code ${code ?? "unknown"}.`)));
  });
}

async function putObject(key, cacheControl) {
  await runWrangler([
    "r2", "object", "put", `${bucket}/${key}`, "--remote", "--file", output,
    "--content-type", "application/json", "--cache-control", cacheControl,
  ]);
}

console.log(`Publishing history: ${publicBaseUrl}/${historyKey}`);
const historyText = await fetchRemoteText(`${publicBaseUrl}/${historyKey}`);
if (historyText === outputText) {
  console.log("History snapshot already exists; skipping.");
} else if (historyText === null) {
  await putObject(historyKey, "public, max-age=31536000, immutable");
} else {
  throw new Error(`History snapshot ${historyKey} already exists with different content.`);
}
if (remoteCatalogText === outputText) {
  console.log("Current catalog already matches; skipping.");
} else {
  console.log(`Publishing current catalog: ${publicBaseUrl}/catalog.json`);
  await putObject("catalog.json", "no-store");
}
console.log("R2 publish complete.");
