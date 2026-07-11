import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { resolve } from "node:path";

const args = process.argv.slice(2);
const tauriBin = resolve("node_modules/.bin/tauri");

function run(command, commandArgs) {
  const child = spawn(command, commandArgs, { stdio: "inherit" });
  child.on("error", (error) => {
    console.error(error.message);
    process.exit(1);
  });
  child.on("exit", (code) => process.exit(code ?? 1));
}

async function reservePort() {
  const server = createServer();
  await new Promise((resolvePromise, reject) => {
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port: 0 }, resolvePromise);
  });
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Could not reserve a localhost port.");
  const { port } = address;
  await new Promise((resolvePromise, reject) => server.close((error) => error ? reject(error) : resolvePromise()));
  return port;
}

async function runDev() {
  const port = await reservePort();
  const vite = spawn(process.execPath, [
    "node_modules/vite/bin/vite.js",
    "--host", "127.0.0.1",
    "--port", String(port),
    "--strictPort",
  ], { stdio: "inherit" });
  const override = JSON.stringify({
    build: {
      beforeDevCommand: null,
      devUrl: `http://127.0.0.1:${port}`,
    },
  });
  const tauri = spawn(tauriBin, ["dev", "--config", override, ...args.slice(1)], { stdio: "inherit" });

  let stopping = false;
  const stop = (code = 0) => {
    if (stopping) return;
    stopping = true;
    vite.kill("SIGTERM");
    tauri.kill("SIGTERM");
    setTimeout(() => process.exit(code), 250).unref();
  };
  process.on("SIGINT", () => stop());
  process.on("SIGTERM", () => stop());
  vite.on("error", (error) => {
    console.error(`Could not start Vite: ${error.message}`);
    stop(1);
  });
  vite.on("exit", (code) => {
    if (!stopping && code !== 0) stop(code ?? 1);
  });
  tauri.on("error", (error) => {
    console.error(`Could not start Tauri: ${error.message}`);
    stop(1);
  });
  tauri.on("exit", (code) => stop(code ?? 1));
}

if (args[0] === "dev") {
  runDev().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
} else {
  run(tauriBin, args);
}
