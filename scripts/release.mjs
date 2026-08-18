import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const targetDirectory = resolve(root, "src-tauri", "target");
const appBundleName = "CCM Reborn";
const tauriCli = resolve(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
const windowsTarget = "x86_64-pc-windows-msvc";
const macTargets = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
const universalMacTarget = "universal-apple-darwin";
const platform = process.platform;
const command = process.argv[2];
const localMakensisDirectory = resolve(root, ".toolcache", "bin");
const localMakensis = resolve(localMakensisDirectory, "makensis");

function fail(message) {
  console.error(`Release build: ${message}`);
  process.exit(1);
}

function run(executable, args, options = {}) {
  const result = spawnSync(executable, args, {
    cwd: root,
    stdio: "inherit",
    ...options,
  });

  if (result.error) fail(`could not run ${executable}: ${result.error.message}`);
  if (result.status !== 0) process.exit(result.status ?? 1);
}

function tauri(args, options) {
  if (!existsSync(tauriCli)) fail("Tauri CLI is missing; run npm install first.");
  run(process.execPath, [tauriCli, "build", ...args], options);
}

function isBrewPackageInstalled(name) {
  return spawnSync("brew", ["list", "--versions", name], { stdio: "ignore" }).status === 0;
}

function ensureMacWindowsPrerequisites() {
  if (platform !== "darwin") fail("Windows cross-build setup is available only on macOS.");
  if (spawnSync("brew", ["--version"], { stdio: "ignore" }).error) {
    fail("Homebrew is required to cross-build Windows installers: https://brew.sh");
  }

  const llvmPackage = ["llvm", "llvm@20"].find(isBrewPackageInstalled) ?? "llvm";
  const packageNames = isBrewPackageInstalled(llvmPackage) ? [] : [llvmPackage];
  if (!existsSync(localMakensis) && !isBrewPackageInstalled("makensis")) {
    packageNames.push("makensis");
  }

  for (const packageName of packageNames) {
    if (!isBrewPackageInstalled(packageName)) run("brew", ["install", packageName]);
  }

  run("rustup", ["target", "add", windowsTarget]);
  if (spawnSync("cargo", ["xwin", "--version"], { stdio: "ignore" }).status !== 0) {
    run("cargo", ["install", "--locked", "cargo-xwin"]);
  }

  return { llvmPackage };
}

/// Tauri ad-hoc signs the main binary but leaves the app bundle unsealed, and
/// both CCM's extra `ccm` CLI binary and the universal `lipo` merge invalidate
/// that signature. macOS refuses to launch an app with a broken signature on
/// Apple Silicon ("the application can't be opened"), so seal the finished
/// bundle ad-hoc and rebuild the disk image from the signed copy.
function sealMacBundle(bundleDirectory) {
  const app = resolve(bundleDirectory, "macos", `${appBundleName}.app`);
  if (!existsSync(app)) fail(`macOS app bundle is missing at ${app}`);
  run("codesign", ["--force", "--deep", "--sign", "-", app]);
  run("codesign", ["--verify", "--strict", app]);

  const dmgDirectory = resolve(bundleDirectory, "dmg");
  const dmg = existsSync(dmgDirectory)
    ? readdirSync(dmgDirectory).find((name) => name.endsWith(".dmg"))
    : undefined;
  if (!dmg) return;

  const staging = mkdtempSync(resolve(tmpdir(), "ccm-dmg-"));
  try {
    run("cp", ["-R", app, resolve(staging, `${appBundleName}.app`)]);
    run("ln", ["-s", "/Applications", resolve(staging, "Applications")]);
    run("hdiutil", [
      "create", "-volname", appBundleName, "-srcfolder", staging,
      "-ov", "-format", "UDZO", resolve(dmgDirectory, dmg),
    ]);
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

function buildMac() {
  if (platform !== "darwin") {
    fail("macOS bundles must be built on macOS. Run this command on a Mac or use a macOS CI runner.");
  }
  tauri(["--bundles", "app,dmg"]);
  sealMacBundle(resolve(targetDirectory, "release", "bundle"));
}

function buildUniversalMac() {
  if (platform !== "darwin") {
    fail("macOS bundles must be built on macOS. Run this command on a Mac or use a macOS CI runner.");
  }
  run("rustup", ["target", "add", ...macTargets]);

  // Tauri discovers every `src/bin/*` binary and includes it in the macOS
  // bundle. Its universal build creates the app binary, but does not merge
  // those additional binaries, so build CCM's CLI explicitly first.
  for (const target of macTargets) {
    run("cargo", [
      "build",
      "--manifest-path", "src-tauri/Cargo.toml",
      "--release",
      "--target", target,
      "--bin", "ccm",
    ]);
  }
  const universalCli = resolve(targetDirectory, universalMacTarget, "release", "ccm");
  mkdirSync(dirname(universalCli), { recursive: true });
  run("lipo", [
    "-create",
    ...macTargets.map((target) => resolve(targetDirectory, target, "release", "ccm")),
    "-output", universalCli,
  ]);

  tauri(["--target", universalMacTarget, "--bundles", "app,dmg"]);
  sealMacBundle(resolve(targetDirectory, universalMacTarget, "release", "bundle"));
}

function buildWindows() {
  if (platform === "win32") {
    tauri(["--bundles", "nsis"]);
    return;
  }
  if (platform !== "darwin") {
    fail("Windows installers are built natively on Windows; macOS cross-builds are also supported here.");
  }

  const { llvmPackage } = ensureMacWindowsPrerequisites();
  const llvmPrefix = spawnSync("brew", ["--prefix", llvmPackage], { encoding: "utf8" });
  if (llvmPrefix.status !== 0) fail("could not locate Homebrew LLVM after installation.");
  const env = {
    ...process.env,
    PATH: [
      existsSync(localMakensis) ? localMakensisDirectory : undefined,
      resolve(llvmPrefix.stdout.trim(), "bin"),
      process.env.PATH,
    ].filter(Boolean).join(":"),
  };
  tauri(["--runner", "cargo-xwin", "--target", windowsTarget, "--bundles", "nsis"], { env });
}

function buildLinuxInDocker() {
  if (spawnSync("docker", ["--version"], { stdio: "ignore" }).error) {
    fail("Docker is required to build Linux artifacts from macOS. Install and start Docker Desktop first.");
  }
  if (spawnSync("docker", ["info"], { stdio: "ignore" }).status !== 0) {
    fail("Docker is installed but its daemon is not running. Start Docker Desktop first.");
  }

  const dockerfile = resolve(root, "scripts", "Dockerfile.linux-release");
  const image = "ccm-reborn-linux-builder";
  const user = typeof process.getuid === "function" && typeof process.getgid === "function"
    ? `${process.getuid()}:${process.getgid()}`
    : "0:0";
  mkdirSync(targetDirectory, { recursive: true });

  run("docker", ["build", "--platform", "linux/amd64", "--file", dockerfile, "--tag", image, root]);
  run("docker", [
    "run", "--rm", "--platform", "linux/amd64", "--user", user,
    "-e", "HOME=/tmp/ccm-home",
    "-e", "CARGO_HOME=/tmp/ccm-cargo",
    "-e", "RUSTUP_HOME=/usr/local/rustup",
    "-v", `${root}:/source:ro`,
    "-v", `${targetDirectory}:/artifacts`,
    image,
    "bash", "-lc",
    "set -euo pipefail; mkdir /work; cp -a /source/. /work/; cd /work; npm ci; npm run tauri build -- --bundles deb,appimage; mkdir -p /artifacts/release/bundle; cp -a src-tauri/target/release/bundle/. /artifacts/release/bundle/",
  ]);
}

function buildLinux() {
  if (platform === "linux") {
    tauri(["--bundles", "deb,appimage"]);
    return;
  }
  if (platform === "darwin") {
    buildLinuxInDocker();
    return;
  }
  fail("Linux artifacts are built natively on Linux; on macOS this command uses Docker.");
}

function printHelp() {
  console.log(`Usage: node scripts/release.mjs <mac|mac-universal|win|linux|all>

mac           Build macOS .app and .dmg on macOS.
mac-universal Build one macOS .app and .dmg for Apple Silicon and Intel Macs.
win           Build an x64 NSIS installer (native on Windows, cross-built on macOS).
linux         Build x64 .deb and AppImage (native on Linux, Docker on macOS).
all           Build all three release variants from macOS.`);
}

if (["-h", "--help", undefined].includes(command)) {
  printHelp();
} else if (command === "mac") {
  buildMac();
} else if (command === "mac-universal") {
  buildUniversalMac();
} else if (command === "win") {
  buildWindows();
} else if (command === "linux") {
  buildLinux();
} else if (command === "all") {
  if (platform !== "darwin") {
    fail("tauri:all runs on macOS because the macOS bundle requires a Mac build host.");
  }
  buildMac();
  buildWindows();
  buildLinux();
} else if (command === "setup-win") {
  ensureMacWindowsPrerequisites();
} else {
  printHelp();
  process.exitCode = 1;
}
