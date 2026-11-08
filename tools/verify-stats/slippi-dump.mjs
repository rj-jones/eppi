// Writes slippi-js getStats() + getWinners() JSON for each replay.
// Usage: node slippi-dump.mjs <listFile> <outDir>
//   listFile: text file with one .slp path per line.
//   outDir:   one <basename>.json per replay is written here.
// Replays are opened read-only.
import fs from "node:fs";
import path from "node:path";
import { SlippiGame } from "@slippi/slippi-js/node";

const [listFile, outDir] = process.argv.slice(2);
if (!listFile || !outDir) {
  console.error("usage: node slippi-dump.mjs <listFile> <outDir>");
  process.exit(2);
}
fs.mkdirSync(outDir, { recursive: true });
const files = fs.readFileSync(listFile, "utf8").split(/\r?\n/).filter(Boolean);

let totalMs = 0;
for (const file of files) {
  const out = path.join(outDir, path.basename(file).replace(/\.slp$/i, ".json"));
  const t0 = performance.now();
  let result;
  try {
    const game = new SlippiGame(file);
    const stats = game.getStats() ?? null;
    const winners = game.getWinners();
    const latest = game.getLatestFrame();
    const settings = game.getSettings();
    const players = (settings?.players ?? []).map((p) => {
      const post = latest?.players?.[p.playerIndex]?.post;
      return {
        playerIndex: p.playerIndex,
        stocksRemaining: post?.stocksRemaining ?? null,
        percent: post?.percent ?? null,
      };
    });
    result = { file, stats, winners, latestFramePlayers: players };
  } catch (e) {
    result = { file, error: String(e?.stack ?? e) };
  }
  totalMs += performance.now() - t0;
  fs.writeFileSync(out, JSON.stringify(result));
}
console.log(`${files.length} files, ${(totalMs / files.length).toFixed(1)} ms/file (slippi-js)`);
