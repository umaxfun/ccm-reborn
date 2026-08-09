import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, relative, resolve, sep } from "node:path";

// This is intentionally a no-argument bootstrap script. It is only for the
// first publication of the current local library; future release tooling can
// replace it once dev-catalog becomes a small test fixture.
const REPOSITORY_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const LOCAL_CATALOG_PATH = resolve(REPOSITORY_ROOT, "dev-catalog/catalog.json");
const LOCAL_PACKAGE_ROOT = resolve(REPOSITORY_ROOT, "dev-catalog/packages");
const PRODUCTION_CATALOG_PATH = resolve(REPOSITORY_ROOT, "catalog/catalog.json");

const R2_BUCKET = "ccm-reborn";
const PUBLIC_BASE_URL = "https://files.ccm-reborn.mikilabs.io";
const PUBLIC_CATALOG_KEY = "catalog.json";
const HISTORY_PREFIX = "catalog-history";
const ARCHIVE_CACHE_CONTROL = "public, max-age=31536000, immutable";
const CATALOG_CACHE_CONTROL = "no-store";

// Keep the public object names explicit and readable. A ZIP is never
// overwritten: changing its contents means assigning a new version/key.
const RELEASES = {
  "f-yuri-of-the-swarm": { key: "campaigns/hots/yuri-1.09.zip", version: "1.09" },
  "abathur-s-whimsy": { key: "campaigns/hots/abathurs-whimsy-3.3.zip", version: "3.3" },
  "heart-of-the-swarm-randomizer": { key: "campaigns/hots/randomizer-1.0.2.zip", version: "1.0.2" },
  "hots-1hp-paintball": { key: "campaigns/hots/paintball-1.1.3.zip", version: "1.1.3" },
  "hots-ai-allies": { key: "campaigns/hots/ai-allies-4.zip", version: "4" },
  "hots-overmind-mod": { key: "campaigns/hots/overmind-1.02.zip", version: "1.02" },
  "kerrigan-has-gone-rogue-like": { key: "campaigns/hots/kerrigan-rogue-1.06.zip", version: "1.06" },
  "nightmare-difficulty": { key: "campaigns/hots/nightmare-2.02.zip", version: "2.02" },
  "real-scale-heart-of-the-swarm": { key: "campaigns/hots/real-scale-2.8.zip", version: "2.8" },
  "the-swarm-reborn": { key: "campaigns/hots/swarm-reborn-0.71.zip", version: "0.71" },
  "violet-s-hots-rework-mod": { key: "campaigns/hots/violets-rework-1.12.zip", version: "1.12" },
  "heart-of-eyeser-ccm-available-poor-google-translate": { key: "campaigns/hots/heart-of-eyeser-1.01.zip", version: "1.01" },

  "aeon-of-purification": { key: "campaigns/lotv/aeon-of-purification-1.43.zip", version: "1.43" },
  "fight-with-ally": { key: "campaigns/lotv/fight-with-ally-1.9.9.zip", version: "1.9.9" },
  "legacy-of-the-void-gauntlet": { key: "campaigns/lotv/gauntlet-1.0.zip", version: "1.0" },
  "legacy-of-the-xel-naga": { key: "campaigns/lotv/xel-naga-1.11.1.zip", version: "1.11.1" },
  "lotv-1hp-paintball": { key: "campaigns/lotv/paintball-1.1.6.zip", version: "1.1.6" },
  "lotv-chaos": { key: "campaigns/lotv/chaos-1.0.zip", version: "1.0" },
  "nightmare-difficulty-legacy-of-the-void": { key: "campaigns/lotv/nightmare-1.14.zip", version: "1.14" },
  "real-scale-lotv": { key: "campaigns/lotv/real-scale-2.8.zip", version: "2.8" },
  "legacy-of-purifier-1-04-ccm-translated": { key: "campaigns/lotv/legacy-of-purifier-1.04.zip", version: "1.04" },
  "mod-ccm-translated": { key: "campaigns/lotv/ccm-translated-1.00.zip", version: "1.00" },

  "avon-overt-cops": { key: "campaigns/nco/avon-overt-cops-1.6.2.zip", version: "1.6.2" },
  "nco-redux-pro-max-legendary-edition": { key: "campaigns/nco/redux-2.6.zip", version: "2.6" },
  "nova-outlaw-ops": { key: "campaigns/nco/outlaw-ops-1.50.zip", version: "1.50" },

  "coop-ai": { key: "campaigns/wol/coop-ai-4.7-hf2.zip", version: "4.7-hf2" },
  "junker-edition": { key: "campaigns/wol/junker-2.02.zip", version: "2.02" },
  "lings-of-wiberty": { key: "campaigns/wol/lings-of-wiberty-1.25.zip", version: "1.25" },
  "mindhawk-s-gauntlet": { key: "campaigns/wol/mindhawk-gauntlet-1.200.zip", version: "1.200" },
  "moebius-pack": { key: "campaigns/wol/moebius-pack-1.35.zip", version: "1.35" },
  "nightmare-difficulty-wings-of-liberty": { key: "campaigns/wol/nightmare-2.025.zip", version: "2.025" },
  "raynor-has-gone-rogue-like": { key: "campaigns/wol/raynor-rogue-1.09.5.zip", version: "1.09.5" },
  "real-scale-wol": { key: "campaigns/wol/real-scale-2.8.zip", version: "2.8" },
  "wings-of-liberty-human-edition": { key: "campaigns/wol/human-edition-1.1.3.zip", version: "1.1.3" },
  "wings-of-mengsk": { key: "campaigns/wol/wings-of-mengsk-1.07.zip", version: "1.07" },
  "wol-1hp-paintball": { key: "campaigns/wol/paintball-1.1.8.zip", version: "1.1.8" },
  "wol-tychus-edition-nightmare": { key: "campaigns/wol/tychus-edition-1.691.zip", version: "1.691" },
};

const BRANCH_ORDER = new Map([
  ["Wings of Liberty", 0],
  ["Heart of the Swarm", 1],
  ["Legacy of the Void", 2],
  ["Nova Covert Ops", 3],
]);

function publicUrl(key) {
  return `${PUBLIC_BASE_URL}/${key}`;
}

function probeUrl(url) {
  const probe = new URL(url);
  probe.searchParams.set("ccm-publish-probe", `${Date.now()}-${Math.random().toString(16).slice(2)}`);
  return probe.toString();
}

function sameJson(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function catalogWithoutTimestamp(catalog) {
  const { updatedAt: _updatedAt, ...rest } = catalog;
  return rest;
}

function timestampKey(timestamp) {
  return timestamp.replace(/[:.]/g, "-");
}

async function readJson(path, required = true) {
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    if (!required && error?.code === "ENOENT") return null;
    throw new Error(`Could not read ${relative(REPOSITORY_ROOT, path)}: ${error.message}`);
  }
}

async function sha256File(path) {
  const hash = createHash("sha256");
  await new Promise((resolvePromise, reject) => {
    const stream = createReadStream(path);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", reject);
    stream.on("end", resolvePromise);
  });
  return hash.digest("hex");
}

function localPackagePath(packagePath) {
  if (typeof packagePath !== "string" || packagePath.trim() === "") {
    throw new Error("Every local catalog package needs a relative package.path.");
  }
  const path = resolve(dirname(LOCAL_CATALOG_PATH), packagePath);
  const packageRootWithSeparator = `${LOCAL_PACKAGE_ROOT}${sep}`;
  if (path !== LOCAL_PACKAGE_ROOT && !path.startsWith(packageRootWithSeparator)) {
    throw new Error(`Package path escapes dev-catalog/packages: ${packagePath}`);
  }
  return path;
}

function validateReleaseLayout(campaigns) {
  const catalogIds = new Set(campaigns.map((campaign) => campaign.id));
  const releaseIds = Object.keys(RELEASES);
  const missing = releaseIds.filter((id) => !catalogIds.has(id));
  const unmapped = [...catalogIds].filter((id) => !RELEASES[id]);
  if (missing.length || unmapped.length) {
    throw new Error([
      missing.length ? `Release map IDs missing from catalog: ${missing.join(", ")}` : "",
      unmapped.length ? `Catalog IDs missing from release map: ${unmapped.join(", ")}` : "",
    ].filter(Boolean).join("\n"));
  }
  const keys = releaseIds.map((id) => RELEASES[id].key);
  if (new Set(keys).size !== keys.length) throw new Error("Release map has duplicate R2 object keys.");
}

async function createPublishPlan(localCatalog) {
  if (localCatalog.format !== 1 || !Array.isArray(localCatalog.campaigns)) {
    throw new Error("The local catalog must use format 1 and contain campaigns.");
  }
  validateReleaseLayout(localCatalog.campaigns);

  const prepared = [];
  for (const campaign of localCatalog.campaigns) {
    const release = RELEASES[campaign.id];
    const path = localPackagePath(campaign.package?.path);
    const metadata = await stat(path);
    if (!metadata.isFile()) throw new Error(`Package is not a file: ${relative(REPOSITORY_ROOT, path)}`);
    const sha256 = await sha256File(path);
    if (sha256 !== campaign.package.sha256) {
      throw new Error(`SHA-256 mismatch for ${campaign.id}; refusing to publish ${relative(REPOSITORY_ROOT, path)}.`);
    }
    if (metadata.size !== campaign.package.size) {
      throw new Error(`Size mismatch for ${campaign.id}; refusing to publish ${relative(REPOSITORY_ROOT, path)}.`);
    }
    prepared.push({ campaign, release, path, size: metadata.size, sha256 });
  }
  return prepared;
}

function makeProductionCatalog(localCatalog, prepared, existingCatalog) {
  const releaseById = new Map(prepared.map((item) => [item.campaign.id, item.release]));
  const campaigns = localCatalog.campaigns.map((campaign) => {
    const release = releaseById.get(campaign.id);
    return {
      ...campaign,
      version: release.version,
      requirements: {
        campaign: campaign.requirements.campaign,
      },
      package: {
        url: publicUrl(release.key),
        sha256: campaign.package.sha256,
        size: campaign.package.size,
      },
    };
  }).sort((left, right) => (
    (BRANCH_ORDER.get(left.requirements.campaign) ?? Number.MAX_SAFE_INTEGER)
      - (BRANCH_ORDER.get(right.requirements.campaign) ?? Number.MAX_SAFE_INTEGER)
      || left.title.localeCompare(right.title, "en")
      || left.id.localeCompare(right.id, "en")
  ));

  const candidate = {
    format: 1,
    name: "CCM Reborn · Community campaigns",
    updatedAt: "",
    campaigns,
  };
  const unchanged = existingCatalog && sameJson(
    catalogWithoutTimestamp(existingCatalog),
    catalogWithoutTimestamp(candidate),
  );
  candidate.updatedAt = unchanged ? existingCatalog.updatedAt : new Date().toISOString();
  return candidate;
}

async function remoteObjectExists(url, expectedSize) {
  let response;
  try {
    response = await fetch(probeUrl(url), { method: "HEAD", cache: "no-store", redirect: "error" });
  } catch (error) {
    throw new Error(`Could not check ${url}: ${error.message}`);
  }
  if (response.status === 404) return false;
  if (!response.ok) throw new Error(`Could not check ${url}: HTTP ${response.status}.`);
  const contentLength = Number(response.headers.get("content-length"));
  if (!Number.isSafeInteger(contentLength) || contentLength !== expectedSize) {
    throw new Error(`Existing object ${url} has an unexpected size; refusing to overwrite it.`);
  }
  return true;
}

function runWrangler(args) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn("wrangler", args, { cwd: REPOSITORY_ROOT, stdio: "inherit" });
    child.on("error", (error) => reject(new Error(`Could not start Wrangler: ${error.message}`)));
    child.on("exit", (code) => {
      if (code === 0) resolvePromise();
      else reject(new Error(`Wrangler exited with code ${code ?? "unknown"}.`));
    });
  });
}

async function putObject(key, file, cacheControl) {
  await runWrangler([
    "r2", "object", "put", `${R2_BUCKET}/${key}`,
    "--remote",
    "--file", file,
    "--content-type", key.endsWith(".zip") ? "application/zip" : "application/json",
    "--cache-control", cacheControl,
  ]);
}

async function remoteText(url) {
  const response = await fetch(probeUrl(url), { cache: "no-store", redirect: "error" });
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`Could not fetch ${url}: HTTP ${response.status}.`);
  return response.text();
}

async function main() {
  const localCatalog = await readJson(LOCAL_CATALOG_PATH);
  const prepared = await createPublishPlan(localCatalog);
  const existingCatalog = await readJson(PRODUCTION_CATALOG_PATH, false);
  const productionCatalog = makeProductionCatalog(localCatalog, prepared, existingCatalog);
  const productionJson = `${JSON.stringify(productionCatalog, null, 2)}\n`;

  let uploaded = 0;
  for (const item of prepared) {
    const url = publicUrl(item.release.key);
    if (await remoteObjectExists(url, item.size)) {
      console.log(`Exists, skipping: ${item.release.key}`);
      continue;
    }
    console.log(`Uploading: ${item.release.key}`);
    await putObject(item.release.key, item.path, ARCHIVE_CACHE_CONTROL);
    if (!await remoteObjectExists(url, item.size)) {
      throw new Error(`Uploaded object is not available at ${url}.`);
    }
    uploaded += 1;
  }

  await mkdir(dirname(PRODUCTION_CATALOG_PATH), { recursive: true });
  await writeFile(PRODUCTION_CATALOG_PATH, productionJson, "utf8");

  const historyKey = `${HISTORY_PREFIX}/${timestampKey(productionCatalog.updatedAt)}.json`;
  const historyUrl = publicUrl(historyKey);
  const historyText = await remoteText(historyUrl);
  if (historyText === productionJson) {
    console.log(`Exists, skipping: ${historyKey}`);
  } else if (historyText === null) {
    console.log(`Uploading: ${historyKey}`);
    await putObject(historyKey, PRODUCTION_CATALOG_PATH, ARCHIVE_CACHE_CONTROL);
  } else {
    throw new Error(`History object ${historyKey} already exists with different content.`);
  }

  const currentCatalogText = await remoteText(publicUrl(PUBLIC_CATALOG_KEY));
  if (currentCatalogText === productionJson) {
    console.log(`Exists, skipping: ${PUBLIC_CATALOG_KEY}`);
  } else {
    console.log(`Publishing: ${PUBLIC_CATALOG_KEY}`);
    await putObject(PUBLIC_CATALOG_KEY, PRODUCTION_CATALOG_PATH, CATALOG_CACHE_CONTROL);
  }

  console.log(`Done. Uploaded ${uploaded} archive(s); ${prepared.length - uploaded} already existed.`);
  console.log(`Production catalog: ${relative(REPOSITORY_ROOT, PRODUCTION_CATALOG_PATH)}`);
  console.log(`Public catalog: ${publicUrl(PUBLIC_CATALOG_KEY)}`);
}

main().catch((error) => {
  console.error(`Publish failed: ${error.message}`);
  process.exitCode = 1;
});
