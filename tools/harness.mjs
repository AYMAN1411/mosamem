#!/usr/bin/env node
// Runs JavaScript inside the running Mosamem debug app (started with MOSAMEM_HARNESS=1).
// Usage: node tools/harness.mjs '<async function body>'   or   node tools/harness.mjs -f script.js
import { readFileSync } from 'node:fs';

const arg = process.argv[2] === '-f' ? readFileSync(process.argv[3], 'utf8') : process.argv.slice(2).join(' ');
const res = await fetch('http://127.0.0.1:47811/', { method: 'POST', body: arg });
const out = JSON.parse(await res.text());
if (!out.ok) {
  console.error(out.error);
  process.exit(1);
}
console.log(typeof out.value === 'string' ? out.value : JSON.stringify(out.value, null, 2));
