#!/usr/bin/env node
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const exe = process.platform === "win32" ? "viralcraft.exe" : "viralcraft";
const binary = fileURLToPath(new URL(`native/${exe}`, import.meta.url));

function fail(error, human) {
  process.stdout.write(JSON.stringify({ command: null, ok: false, error }) + "\n");
  process.stderr.write(human + "\n");
  process.exit(1);
}

if (!existsSync(binary)) {
  fail(
    "viralcraft is not installed in this folder yet: run npm install",
    "viralcraft: the program is missing. Run `npm install` in this folder first.",
  );
}

const child = spawn(binary, process.argv.slice(2), { stdio: "inherit" });

// Ctrl-C reaches the whole process group, so the child already has it and
// decides whether to stop. Signals sent only to us are passed on.
process.on("SIGINT", () => {});
for (const signal of ["SIGTERM", "SIGHUP"]) {
  process.on(signal, () => child.kill(signal));
}

child.on("error", (err) => {
  fail(`viralcraft could not start: ${err.message}`, `viralcraft: could not start ${binary}: ${err.message}`);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.removeAllListeners(signal);
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
