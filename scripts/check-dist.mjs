#!/usr/bin/env node
// Stops a release whose interface lost its styles. Tailwind's automatic source detection skips
// files a parent folder's .gitignore covers; on one Mac that produced a build with no utility
// classes at all (a bare, unstyled window). Run after `vite build`.
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const dir = new URL("../dist/assets", import.meta.url).pathname;
const css = readdirSync(dir).filter((f) => /^index-.*\.css$/.test(f));
if (!css.length) {
  console.error("check-dist: no dist/assets/index-*.css; run the build first.");
  process.exit(1);
}
const text = css.map((f) => readFileSync(join(dir, f), "utf8")).join("\n");
const needed = [".flex{", ".items-center{", ".ms-auto{", ".text-fg{", ".rounded-lg{", ".gap-2{"];
const missing = needed.filter((c) => !text.includes(c));
if (missing.length || text.length < 30_000) {
  console.error(`check-dist: the interface CSS is missing its utility classes (${text.length} bytes; missing ${missing.join(" ") || "none"}).`);
  console.error("Tailwind didn't scan src/. Check @source in src/index.css and any .gitignore above the project.");
  process.exit(1);
}
console.log(`check-dist: interface CSS ok (${Math.round(text.length / 1024)} KB).`);
