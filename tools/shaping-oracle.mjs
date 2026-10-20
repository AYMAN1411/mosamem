#!/usr/bin/env node
// Checks the engine's shaping (docs/shaping-golden.json, from
// `cargo run -p mosamem-core --example shaping_golden`) against HarfBuzz itself (harfbuzzjs).
// Glyph ids, clusters, advances and offsets must all match. Writes docs/shaping-oracle.json.
import * as hb from 'harfbuzzjs';
import { readFileSync, writeFileSync } from 'node:fs';

const golden = JSON.parse(readFileSync('docs/shaping-golden.json', 'utf8'));
const fonts = new Map();
function fontFor(file, variableWeight) {
  const key = `${file}@${variableWeight}`;
  if (!fonts.has(key)) {
    const blob = new hb.Blob(new Uint8Array(readFileSync(`assets/fonts/${file}`)));
    const font = new hb.Font(new hb.Face(blob, 0));
    if (variableWeight != null) font.setVariations([new hb.Variation('wght', variableWeight)]);
    fonts.set(key, font);
  }
  return fonts.get(key);
}

// the engine's clusters are UTF-8 byte offsets; HarfBuzz here gets UTF-16
function byteToUtf16(text) {
  const map = new Map();
  let bytes = 0;
  let units = 0;
  for (const ch of text) {
    map.set(bytes, units);
    bytes += new TextEncoder().encode(ch).length;
    units += ch.length;
  }
  map.set(bytes, units);
  return map;
}

const results = [];
let failed = 0;
for (const g of golden) {
  const font = fontFor(g.font, g.variableWeight);
  const buf = new hb.Buffer();
  buf.addText(g.text);
  buf.setDirection(g.rtl ? hb.Direction.RTL : hb.Direction.LTR);
  buf.guessSegmentProperties();
  hb.shape(font, buf);
  const infos = buf.getGlyphInfos();
  const pos = buf.getGlyphPositions();
  const want = infos.map((info, i) => [info.codepoint, info.cluster, pos[i].xAdvance, pos[i].xOffset, pos[i].yOffset]);
  const map = byteToUtf16(g.text);
  const got = g.glyphs.map(([id, c, a, dx, dy]) => [id, map.get(c), a, dx, dy]);
  const same = JSON.stringify(want) === JSON.stringify(got);
  if (!same) failed++;
  results.push({ font: g.font, text: g.text, glyphs: got.length, match: same, ...(same ? {} : { engine: got, harfbuzz: want }) });
}
const summary = { harfbuzz: hb.versionString(), cases: results.length, matching: results.length - failed, failed };
writeFileSync('docs/shaping-oracle.json', JSON.stringify({ summary, results }, null, 2) + '\n');
console.log(JSON.stringify(summary));
for (const r of results.filter((r) => !r.match).slice(0, 5)) console.log('mismatch', r.font, r.text, JSON.stringify(r.engine), JSON.stringify(r.harfbuzz));
process.exit(failed ? 1 : 0);
