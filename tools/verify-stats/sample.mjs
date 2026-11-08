// Picks a deterministic sample of .slp files from a folder and writes their paths, one per line.
// Usage: node sample.mjs <replayDir> <count> <outList>
// The sample is: `count` files spread evenly by name (≈ by date) plus the 30 smallest
// (short games, quit-outs) and the 10 largest files.
import fs from "node:fs";
import path from "node:path";

const [dir, countArg, outList] = process.argv.slice(2);
if (!dir || !countArg || !outList) {
  console.error("usage: node sample.mjs <replayDir> <count> <outList>");
  process.exit(2);
}
const count = Number(countArg);
const files = fs
  .readdirSync(dir)
  .filter((f) => f.toLowerCase().endsWith(".slp"))
  .sort()
  .map((f) => {
    const full = path.join(dir, f);
    return { full, size: fs.statSync(full).size };
  });

const picked = new Set();
const step = files.length / count;
for (let i = 0; i < count && i * step < files.length; i++) {
  picked.add(files[Math.floor(i * step)].full);
}
const bySize = [...files].sort((a, b) => a.size - b.size);
bySize.slice(0, 30).forEach((f) => picked.add(f.full));
bySize.slice(-10).forEach((f) => picked.add(f.full));

fs.mkdirSync(path.dirname(path.resolve(outList)), { recursive: true });
fs.writeFileSync(outList, [...picked].join("\n") + "\n");
console.log(`${picked.size} files written to ${outList} (from ${files.length})`);
