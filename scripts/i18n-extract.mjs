#!/usr/bin/env node
/**
 * Lists every interface string used with t() / tn() into src/locales/en.json, and reports
 * which keys each translation is missing or no longer uses.
 *
 *   node scripts/i18n-extract.mjs            write en.json, report coverage
 *   node scripts/i18n-extract.mjs --check    exit 1 if a translation misses keys
 *   node scripts/i18n-extract.mjs --prune    also drop unused keys from translations
 *
 * en.json maps each key to its English text. tn() keys are "one|other" and map to
 * { one, other }; translations give the plural forms their language needs.
 */
import { readFileSync, writeFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const src = join(root, "src");
const out = join(src, "locales");

function files(dir) {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) return name === "locales" || name === "__tests__" ? [] : files(p);
    return /\.(ts|tsx)$/.test(name) && !/\.test\.tsx?$/.test(name) ? [p] : [];
  });
}

/** Reads a string literal starting at i ("…", '…' or `…` without ${}). */
function literal(code, i) {
  const q = code[i];
  if (q !== '"' && q !== "'" && q !== "`") return null;
  let s = "";
  for (let j = i + 1; j < code.length; j++) {
    const c = code[j];
    if (c === "\\") {
      const n = code[++j];
      s += n === "n" ? "\n" : n === "t" ? "\t" : n === "u" ? String.fromCharCode(parseInt(code.slice(j + 1, (j += 4) + 1), 16)) : n;
      continue;
    }
    if (q === "`" && c === "$" && code[j + 1] === "{") return { dynamic: true, end: j };
    if (c === q) return { value: s, end: j + 1 };
    s += c;
  }
  return null;
}

const skipSpace = (code, i) => {
  while (i < code.length && /\s/.test(code[i])) i++;
  return i;
};

/** End of a template literal starting at i, nested ${…} included. */
function templateEnd(code, i) {
  for (let j = i + 1, depth = 0; j < code.length; j++) {
    const c = code[j];
    if (c === "\\") j++;
    else if (!depth && c === "`") return j + 1;
    else if (c === "$" && code[j + 1] === "{") (depth++, j++);
    else if (depth && c === "{") depth++;
    else if (depth && c === "}") depth--;
  }
  return code.length;
}

/** Skips one argument expression (the count in tn) up to the next top-level comma. */
function skipArg(code, i) {
  let depth = 0;
  for (; i < code.length; i++) {
    const c = code[i];
    if (c === '"' || c === "'" || c === "`") {
      const l = literal(code, i);
      if (!l || l.dynamic) return -1;
      i = l.end - 1;
    } else if ("([{".includes(c)) depth++;
    else if (")]}".includes(c)) {
      if (!depth) return -1;
      depth--;
    } else if (c === "," && !depth) return i + 1;
  }
  return -1;
}

const keys = new Map();
const problems = [];

for (const file of files(src)) {
  const code = readFileSync(file, "utf8");
  const at = (i) => `${relative(root, file)}:${code.slice(0, i).split("\n").length}`;
  for (const m of code.matchAll(/(?<![\w.$])(tn?)\(/g)) {
    let i = skipSpace(code, m.index + m[0].length);
    if (m[1] === "t") {
      const l = literal(code, i);
      if (!l) continue; // t(variable): a key from a table, found where the table is
      if (l.dynamic) problems.push(`${at(m.index)}  t() with a template literal; use {placeholders}`);
      else keys.set(l.value, l.value);
    } else {
      i = skipArg(code, i);
      if (i < 0) continue;
      const one = literal(code, skipSpace(code, i));
      if (!one || one.dynamic) continue;
      const comma = skipSpace(code, one.end);
      if (code[comma] !== ",") continue;
      const other = literal(code, skipSpace(code, comma + 1));
      if (!other || other.dynamic) {
        problems.push(`${at(m.index)}  tn() forms must be plain strings`);
        continue;
      }
      keys.set(`${one.value}|${other.value}`, { one: one.value, other: other.value });
    }
  }
}

/**
 * Tables of labels shown with t(label): strings listed in `// i18n:` comments or in arrays/objects
 * marked with `/* i18n *\/` aren't visible to the scan above, so they're listed explicitly.
 */
for (const file of files(src)) {
  const code = readFileSync(file, "utf8");
  for (const m of code.matchAll(/\/\*\s*i18n\s*\*\/\s*([[{])/g)) {
    let depth = 0;
    let i = m.index + m[0].length - 1;
    for (; i < code.length; i++) {
      const c = code[i];
      if (c === '"' || c === "'" || c === "`") {
        const l = literal(code, i);
        if (!l) break;
        if (l.dynamic) {
          i = templateEnd(code, i) - 1; // `${MOD}N` and the like: not text
          continue;
        }
        // property keys and ids (followed by ":" or lower-case identifiers) aren't text
        const next = code[skipSpace(code, l.end)];
        if (next !== ":" && /[A-Z ]/.test(l.value)) keys.set(l.value, l.value);
        i = l.end - 1;
      } else if ("([{".includes(c)) depth++;
      else if (")]}".includes(c) && --depth === 0) break;
    }
  }
}

const sorted = Object.fromEntries([...keys].sort(([a], [b]) => a.localeCompare(b)));
writeFileSync(join(out, "en.json"), JSON.stringify(sorted, null, 2) + "\n");
console.log(`${keys.size} keys → src/locales/en.json`);

const prune = process.argv.includes("--prune");
let missingAny = false;
for (const name of existsSync(out) ? readdirSync(out) : []) {
  if (!name.endsWith(".json") || name === "en.json") continue;
  const path = join(out, name);
  const cat = JSON.parse(readFileSync(path, "utf8"));
  const missing = [...keys.keys()].filter((k) => !(k in cat) || cat[k] === "");
  const unused = Object.keys(cat).filter((k) => !keys.has(k));
  missingAny ||= missing.length > 0;
  console.log(`${name.padEnd(8)} ${keys.size - missing.length}/${keys.size}${missing.length ? `, missing ${missing.length}` : ""}${unused.length ? `, unused ${unused.length}` : ""}`);
  if (prune && unused.length) {
    for (const k of unused) delete cat[k];
    writeFileSync(path, JSON.stringify(cat, null, 2) + "\n");
  }
}

for (const p of problems) console.warn("warning:", p);
if (process.argv.includes("--check") && (missingAny || problems.length)) process.exit(1);
