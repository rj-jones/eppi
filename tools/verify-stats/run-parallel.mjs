// Runs slippi-dump.mjs over a list file in N parallel Node processes.
// Usage: node run-parallel.mjs <listFile> <outDir> [processes=8]
import fs from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";

const [listFile, outDir, procArg] = process.argv.slice(2);
const procs = Number(procArg ?? 8);
const files = fs.readFileSync(listFile, "utf8").split(/\r?\n/).filter(Boolean);
const chunkDir = path.join(path.dirname(listFile), "chunks");
fs.mkdirSync(chunkDir, { recursive: true });
const runs = [];
for (let i = 0; i < procs; i++) {
  const chunk = files.filter((_, j) => j % procs === i);
  const chunkFile = path.join(chunkDir, `chunk${i}.txt`);
  fs.writeFileSync(chunkFile, chunk.join("\n") + "\n");
  runs.push(
    new Promise((resolve) => {
      const child = spawn(process.execPath, ["--no-warnings", "slippi-dump.mjs", chunkFile, outDir], { stdio: "inherit" });
      child.on("exit", resolve);
    }),
  );
}
await Promise.all(runs);
console.log(`done: ${files.length} files`);
