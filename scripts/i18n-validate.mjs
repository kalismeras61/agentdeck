#!/usr/bin/env node
// Checks translations against src/locales/en.json: every key present, the same {placeholders},
// and every plural form the language needs (Intl.PluralRules) for tn() keys.
//   node scripts/i18n-validate.mjs [tr de …]     (default: every catalog)
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const dir = new URL("../src/locales", import.meta.url).pathname;
const en = JSON.parse(readFileSync(join(dir, "en.json"), "utf8"));
const ids = process.argv.slice(2).length
  ? process.argv.slice(2)
  : readdirSync(dir).filter((f) => f.endsWith(".json") && f !== "en.json").map((f) => f.replace(".json", ""));

const holes = (s) => [...String(s).matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");
let bad = 0;
for (const id of ids) {
  const cat = JSON.parse(readFileSync(join(dir, `${id}.json`), "utf8"));
  const forms = new Intl.PluralRules(id).resolvedOptions().pluralCategories;
  const errors = [];
  for (const [key, source] of Object.entries(en)) {
    const v = cat[key];
    if (v === undefined || v === "") {
      errors.push(`missing: ${key}`);
      continue;
    }
    if (typeof source === "string") {
      if (typeof v !== "string") errors.push(`should be a string: ${key}`);
      else if (holes(v) !== holes(source)) errors.push(`placeholders {${holes(v)}} ≠ {${holes(source)}}: ${key}`);
    } else {
      if (typeof v !== "object") {
        errors.push(`should be plural forms {${forms.join(", ")}}: ${key}`);
        continue;
      }
      const want = holes(source.other);
      for (const f of forms) {
        if (!v[f]) errors.push(`plural form "${f}" missing: ${key}`);
        else if (holes(v[f].replace("{n}", "")) !== holes(source.other.replace("{n}", "")))
          errors.push(`placeholders in "${f}" {${holes(v[f])}} ≠ {${want}}: ${key}`);
      }
    }
  }
  for (const key of Object.keys(cat)) if (!(key in en)) errors.push(`not used: ${key}`);
  console.log(`${id}: ${errors.length ? `${errors.length} problems` : "ok"} (${Object.keys(en).length} keys, plural forms ${forms.join("/")})`);
  for (const e of errors.slice(0, 40)) console.log("  " + e);
  if (errors.length > 40) console.log(`  … ${errors.length - 40} more`);
  bad += errors.length;
}
process.exit(bad ? 1 : 0);
