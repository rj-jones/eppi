// Compares slippi-js output (slippi-dump.mjs) with eppi-stats output (dump_stats example).
// Usage: node compare.mjs <slippiDir> <eppiDir> [--examples N]
// Numbers match if equal within a relative tolerance of 1e-9; exact (bitwise) equality is
// reported separately. Fields only eppi has (extras) are ignored; a field slippi-js leaves
// undefined matches an eppi null.
import fs from "node:fs";
import path from "node:path";

const args = process.argv.slice(2);
const [slippiDir, eppiDir] = args;
const exIdx = args.indexOf("--examples");
const maxExamples = exIdx >= 0 ? Number(args[exIdx + 1]) : 3;
if (!slippiDir || !eppiDir) {
  console.error("usage: node compare.mjs <slippiDir> <eppiDir> [--examples N]");
  process.exit(2);
}

const EPPI_ONLY = new Set(["killedBy", "killMoveId"]);
const TOL = 1e-9;

let numericCompared = 0;
let numericInexact = 0;
const mismatches = new Map(); // pattern -> { count, files:Set, examples:[] }

function record(file, p, a, b) {
  const pattern = p.replace(/\[\d+\]/g, "[]");
  let m = mismatches.get(pattern);
  if (!m) mismatches.set(pattern, (m = { count: 0, files: new Set(), examples: [] }));
  m.count++;
  m.files.add(file);
  if (m.examples.length < maxExamples) m.examples.push({ file: path.basename(file), path: p, slippi: a, eppi: b });
}

function isNil(v) {
  return v === undefined || v === null;
}

function cmp(file, p, a, b) {
  if (isNil(a) && isNil(b)) return;
  if (typeof a === "number" && typeof b === "number") {
    numericCompared++;
    if (a === b) return;
    numericInexact++;
    if (Math.abs(a - b) <= TOL * Math.max(1, Math.abs(a))) return;
    return record(file, p, a, b);
  }
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) record(file, p + ".length", a.length, b.length);
    const n = Math.min(a.length, b.length);
    for (let i = 0; i < n; i++) cmp(file, `${p}[${i}]`, a[i], b[i]);
    return;
  }
  if (a && b && typeof a === "object" && typeof b === "object") {
    const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
    for (const k of keys) {
      if (EPPI_ONLY.has(k) && !(k in a)) continue;
      cmp(file, p ? `${p}.${k}` : k, a[k], b[k]);
    }
    return;
  }
  if (a !== b) record(file, p, a, b);
}

const files = fs.readdirSync(slippiDir).filter((f) => f.endsWith(".json"));
let compared = 0;
let identical = 0;
const errors = [];
for (const f of files) {
  const s = JSON.parse(fs.readFileSync(path.join(slippiDir, f), "utf8"));
  const ePath = path.join(eppiDir, f);
  if (!fs.existsSync(ePath)) {
    errors.push(`${f}: no eppi output`);
    continue;
  }
  const e = JSON.parse(fs.readFileSync(ePath, "utf8"));
  if (s.error || e.error) {
    errors.push(`${f}: slippi-js error=${s.error ? s.error.split("\n")[0] : "-"} eppi error=${e.error ?? "-"}`);
    continue;
  }
  compared++;
  const before = [...mismatches.values()].reduce((n, m) => n + m.count, 0);
  const { winners: eWinners, players: ePlayers, ...eStats } = e.stats;
  cmp(s.file, "stats", s.stats, eStats);
  cmp(s.file, "winners", s.winners, eWinners);
  if (s.stats?.gameComplete) {
    // Players without data on the last frame (eliminated in 3-4 player games) are skipped:
    // slippi-js has nothing to compare; eppi reports their last frame with data.
    s.latestFramePlayers.forEach((p, i) => {
      if (p.stocksRemaining === null) return;
      const expected = { playerIndex: p.playerIndex, finalStocks: p.stocksRemaining, finalPercent: p.percent };
      cmp(s.file, `players[${i}]`, expected, ePlayers[i]);
    });
  }
  const after = [...mismatches.values()].reduce((n, m) => n + m.count, 0);
  if (after === before) identical++;
}

console.log(`files compared: ${compared}, fully matching: ${identical}, with differences: ${compared - identical}`);
console.log(`numbers compared: ${numericCompared}, not bitwise equal: ${numericInexact}`);
if (errors.length) {
  console.log(`\nerrors (${errors.length}):`);
  errors.slice(0, 20).forEach((e) => console.log("  " + e));
}
if (mismatches.size) {
  console.log("\nmismatches by field:");
  const sorted = [...mismatches.entries()].sort((a, b) => b[1].files.size - a[1].files.size);
  for (const [pattern, m] of sorted) {
    console.log(`  ${pattern}: ${m.count} values in ${m.files.size} files`);
    for (const ex of m.examples) console.log(`      ${ex.file} ${ex.path}: slippi=${JSON.stringify(ex.slippi)} eppi=${JSON.stringify(ex.eppi)}`);
  }
}
