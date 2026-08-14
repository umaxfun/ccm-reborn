import { readdir, readFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const limit = 500;
const extensions = new Set([".rs", ".ts", ".css", ".mjs"]);
const ignored = new Set([".git", "node_modules", "target", "dist", "gen"]);

async function walk(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    if (ignored.has(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await walk(path));
    else if (extensions.has(entry.name.slice(entry.name.lastIndexOf(".")))) files.push(path);
  }
  return files;
}

const files = await walk(root);
const oversized = [];
for (const file of files) {
  const lines = (await readFile(file, "utf8")).split("\n").length;
  if (lines > limit) oversized.push([relative(root, file), lines]);
}

if (oversized.length) {
  for (const [file, lines] of oversized) console.error(`${file}: ${lines} lines (limit ${limit})`);
  process.exitCode = 1;
}
