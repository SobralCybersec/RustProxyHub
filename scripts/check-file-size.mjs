import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const ROOTS = ["src", "tests", "scripts", "src-tauri/src", "src-tauri/tests"];
const SOURCE_EXTENSIONS = new Set([".cjs", ".js", ".mjs", ".rs", ".ts", ".vue"]);
const HARD_LIMIT = 1_000;
const REVIEW_LIMIT = 500;

async function sourceFiles(root) {
  const files = [];

  async function visit(relativeDir) {
    const absoluteDir = path.join(root, relativeDir);
    let entries;
    try {
      entries = await readdir(absoluteDir, { withFileTypes: true });
    } catch (error) {
      if (error?.code === "ENOENT") return;
      throw error;
    }

    for (const entry of entries) {
      const relativePath = path.join(relativeDir, entry.name);
      if (entry.isDirectory()) {
        await visit(relativePath);
        continue;
      }
      if (!entry.isFile()) continue;
      if (!SOURCE_EXTENSIONS.has(path.extname(entry.name))) continue;
      files.push(path.join(root, relativePath));
    }
  }

  await visit("");
  return files;
}

const files = (await Promise.all(ROOTS.map(sourceFiles))).flat().sort();
const oversized = [];
const review = [];

for (const file of files) {
  const content = await readFile(file, "utf8");
  const lines = content === "" ? 0 : content.split(/\r?\n/).length;
  if (lines > HARD_LIMIT) oversized.push({ file, lines });
  else if (lines > REVIEW_LIMIT) review.push({ file, lines });
}

for (const { file, lines } of review) {
  console.log(`review: ${file} (${lines} lines)`);
}

if (oversized.length === 0) {
  console.log(`file-size gate passed: ${files.length} handwritten source files <= ${HARD_LIMIT} lines`);
  process.exit(0);
}

for (const { file, lines } of oversized) {
  console.error(`error: ${file} has ${lines} lines (limit ${HARD_LIMIT})`);
}
process.exit(1);
