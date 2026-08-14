import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const paths = {
  packageJson: resolve(root, "package.json"),
  packageLock: resolve(root, "package-lock.json"),
  cargoToml: resolve(root, "src-tauri", "Cargo.toml"),
  cargoLock: resolve(root, "src-tauri", "Cargo.lock"),
  tauriConfig: resolve(root, "src-tauri", "tauri.conf.json"),
};
const semver = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;

function fail(message) {
  console.error(`Version bump: ${message}`);
  process.exit(1);
}

function usage() {
  console.log(`Usage: npm run version:bump -- <patch|minor|major|version> [--dry-run]

Examples:
  npm run version:bump             # patch by default
  npm run version:bump -- patch
  npm run version:bump -- minor
  npm run version:bump -- 1.2.3
  npm run version:bump -- 1.2.3-rc.1 --dry-run
  node scripts/bump-version.mjs --check`);
}

function json(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    fail(`could not parse ${path}: ${error.message}`);
  }
}

function replaceOnce(source, pattern, replacement, label) {
  const flags = [...new Set([...pattern.flags, "g"])].join("");
  const matches = [...source.matchAll(new RegExp(pattern.source, flags))];
  if (matches.length !== 1) fail(`could not identify exactly one ${label} version.`);
  return source.replace(pattern, replacement);
}

function nextVersion(current, kind) {
  const match = current.match(semver);
  if (!match) fail(`current version ${current} is not valid semver.`);
  const [major, minor, patch] = match.slice(1, 4).map(Number);
  if (kind === "major") return `${major + 1}.0.0`;
  if (kind === "minor") return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

const args = process.argv.slice(2);
if (args.includes("--help") || args.includes("-h")) {
  usage();
  process.exit(0);
}

const dryRun = args.includes("--dry-run");
const check = args.includes("--check");
const values = args.filter((arg) => arg !== "--dry-run" && arg !== "--check");
if (check && values.length !== 0) {
  usage();
  process.exit(1);
}
if (values.length > 1) {
  usage();
  process.exit(1);
}

const packageJson = json(paths.packageJson);
const packageLock = json(paths.packageLock);
const tauriConfig = json(paths.tauriConfig);
const cargoToml = readFileSync(paths.cargoToml, "utf8");
const cargoLock = readFileSync(paths.cargoLock, "utf8");
const cargoTomlVersion = cargoToml.match(/^version = "([^"]+)"$/m)?.[1];
const cargoLockVersion = cargoLock.match(/^\[\[package\]\]\nname = "ccm-reborn"\nversion = "([^"]+)"$/m)?.[1];
const currentVersions = {
  "package.json": packageJson.version,
  "package-lock.json": packageLock.version,
  "package-lock.json packages['']": packageLock.packages?.[""]?.version,
  "src-tauri/Cargo.toml": cargoTomlVersion,
  "src-tauri/Cargo.lock": cargoLockVersion,
  "src-tauri/tauri.conf.json": tauriConfig.version,
};
const uniqueVersions = new Set(Object.values(currentVersions));
if (uniqueVersions.size !== 1 || [...uniqueVersions].some((version) => typeof version !== "string")) {
  fail(`versions are out of sync:\n${Object.entries(currentVersions).map(([path, version]) => `  ${path}: ${version ?? "missing"}`).join("\n")}`);
}

const current = packageJson.version;
if (check) {
  console.log(`CCM Reborn manifests are synchronized at ${current}.`);
  process.exit(0);
}
const requested = values[0] ?? "patch";
const next = ["patch", "minor", "major"].includes(requested) ? nextVersion(current, requested) : requested;
if (!semver.test(next)) fail(`expected a semantic version or patch/minor/major, received ${requested}.`);
if (next === current) fail(`version is already ${next}.`);

packageJson.version = next;
packageLock.version = next;
packageLock.packages[""].version = next;
tauriConfig.version = next;
const nextCargoToml = replaceOnce(cargoToml, /^version = "[^"]+"$/m, `version = "${next}"`, "Cargo.toml");
const nextCargoLock = replaceOnce(
  cargoLock,
  /^(\[\[package\]\]\nname = "ccm-reborn"\nversion = ")[^"]+("$)/m,
  `$1${next}$2`,
  "Cargo.lock",
);

if (dryRun) {
  console.log(`Would bump CCM Reborn from ${current} to ${next}.`);
  process.exit(0);
}

writeFileSync(paths.packageJson, `${JSON.stringify(packageJson, null, 2)}\n`);
writeFileSync(paths.packageLock, `${JSON.stringify(packageLock, null, 2)}\n`);
writeFileSync(paths.cargoToml, nextCargoToml);
writeFileSync(paths.cargoLock, nextCargoLock);
writeFileSync(paths.tauriConfig, `${JSON.stringify(tauriConfig, null, 2)}\n`);
console.log(`Bumped CCM Reborn from ${current} to ${next}.`);
