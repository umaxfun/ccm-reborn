import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const targetDirectory = resolve(root, "src-tauri", "target");
const tauriCli = resolve(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
const windowsTarget = "x86_64-pc-windows-msvc";
const platform = process.platform;
const command = process.argv[2];

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

  for (const packageName of ["llvm", "nsis"]) {
    if (!isBrewPackageInstalled(packageName)) run("brew", ["install", packageName]);
  }

  run("rustup", ["target", "add", windowsTarget]);
  if (spawnSync("cargo", ["xwin", "--version"], { stdio: "ignore" }).status !== 0) {
    run("cargo", ["install", "--locked", "cargo-xwin"]);
  }
}

function buildMac() {
  if (platform !== "darwin") {
    fail("macOS bundles must be built on macOS. Run this command on a Mac or use a macOS CI runner.");
  }
  tauri(["--bundles", "app,dmg"]);
}

function buildWindows() {
  if (platform === "win32") {
    tauri(["--bundles", "nsis"]);
    return;
  }
  if (platform !== "darwin") {
    fail("Windows installers are built natively on Windows; macOS cross-builds are also supported here.");
  }

  ensureMacWindowsPrerequisites();
  const llvmPrefix = spawnSync("brew", ["--prefix", "llvm"], { encoding: "utf8" });
  if (llvmPrefix.status !== 0) fail("could not locate Homebrew LLVM after installation.");
  const env = {
    ...process.env,
    PATH: [resolve(llvmPrefix.stdout.trim(), "bin"), process.env.PATH].filter(Boolean).join(":"),
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
  console.log(`Usage: node scripts/release.mjs <mac|win|linux|all>

mac    Build macOS .app and .dmg on macOS.
win    Build an x64 NSIS installer (native on Windows, cross-built on macOS).
linux  Build x64 .deb and AppImage (native on Linux, Docker on macOS).
all    Build all three release variants from macOS.`);
}

if (["-h", "--help", undefined].includes(command)) {
  printHelp();
} else if (command === "mac") {
  buildMac();
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
