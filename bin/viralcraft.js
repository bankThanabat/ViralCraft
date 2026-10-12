#!/usr/bin/env node
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const exe = process.platform === "win32" ? "viralcraft.exe" : "viralcraft";
const binary = fileURLToPath(new URL(`native/${exe}`, import.meta.url));

function fail(error, human) {
  process.stdout.write(JSON.stringify({ command: null, ok: false, error }) + "\n");
  process.stderr.write(human + "\n");
  process.exitCode = 1;
}

if (existsSync(binary)) {
  run();
} else {
  fail(
    "viralcraft is not installed in this folder yet: run npm install",
    "viralcraft: the program is missing. Run `npm install` in this folder first.",
  );
}

function run() {
  const child = spawn(binary, process.argv.slice(2), { stdio: "inherit" });

  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
    process.on(signal, () => child.kill(signal));
  }

  child.on("error", (err) => {
    fail(
      `viralcraft could not start: ${err.message}`,
      `viralcraft: could not start ${binary}: ${err.message}`,
    );
  });

  child.on("exit", (code, signal) => {
    if (signal) {
      process.removeAllListeners(signal);
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 1);
  });
}
