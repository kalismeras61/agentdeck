#!/usr/bin/env node
// One-off: physical Tailwind direction classes → logical ones, so layouts mirror in Arabic.
// Centering pairs (left-1/2 with -translate-x-1/2) are left as they are.
import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const src = new URL("../src", import.meta.url).pathname;
const files = (d) =>
  readdirSync(d).flatMap((n) => {
    const p = join(d, n);
    return statSync(p).isDirectory() ? files(p) : /\.tsx?$/.test(n) && !/\.test\./.test(n) ? [p] : [];
  });

const B = `(^|[\\s"'\`{(:])`; // class token start (also after a variant like hover:)
const rules = [
  [new RegExp(`${B}(-?)ml-`, "g"), "$1$2ms-"],
  [new RegExp(`${B}(-?)mr-`, "g"), "$1$2me-"],
  [new RegExp(`${B}pl-`, "g"), "$1ps-"],
  [new RegExp(`${B}pr-`, "g"), "$1pe-"],
  [new RegExp(`${B}(-?)left-(?!1/2)`, "g"), "$1$2start-"],
  [new RegExp(`${B}(-?)right-`, "g"), "$1$2end-"],
  [new RegExp(`${B}text-left(?![\\w-])`, "g"), "$1text-start"],
  [new RegExp(`${B}text-right(?![\\w-])`, "g"), "$1text-end"],
  [new RegExp(`${B}border-l(?=[\\s"'\`-]|$)`, "g"), "$1border-s"],
  [new RegExp(`${B}border-r(?=[\\s"'\`-]|$)`, "g"), "$1border-e"],
  [new RegExp(`${B}rounded-br-`, "g"), "$1rounded-ee-"],
  [new RegExp(`${B}rounded-bl-`, "g"), "$1rounded-es-"],
  [new RegExp(`${B}rounded-tr-`, "g"), "$1rounded-se-"],
  [new RegExp(`${B}rounded-tl-`, "g"), "$1rounded-ss-"],
];

let changed = 0;
for (const f of files(src)) {
  const code = readFileSync(f, "utf8");
  // only touch string literals that look like class lists
  const next = code.replace(/(["'`])((?:(?!\1)[^\\\n]|\\.)*)\1/g, (lit, q, body) => {
    if (!/\s/.test(body) && !/^[\w:.\/\[\]-]+$/.test(body)) return lit; // e.g. an id like `pl-${seq}`
    if (!/(^|\s)(flex|grid|block|absolute|relative|text-|border|rounded|p[xylr]?-|m[xylr]?-|gap-|w-|h-|ml-|mr-|pl-|pr-|left-|right-|items-|shrink|truncate|font-)/.test(body)) return lit;
    let b = body;
    for (const [re, to] of rules) b = b.replace(re, to);
    return q + b + q;
  });
  if (next !== code) {
    writeFileSync(f, next);
    changed++;
  }
}
console.log(`${changed} files changed`);
