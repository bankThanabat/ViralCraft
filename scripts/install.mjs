// npm postinstall: put a working viralcraft binary in bin/native and prepare
// the folder. Every step is safe to rerun.
import { spawnSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readlinkSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gunzipSync } from "node:zlib";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const win = process.platform === "win32";
const exe = win ? "viralcraft.exe" : "viralcraft";
const nativeDir = join(root, "bin", "native");
const binary = join(nativeDir, exe);
const target = `${process.platform}-${process.arch}`;
const released = ["darwin-arm64", "darwin-x64", "linux-x64", "linux-arm64", "win32-x64"];
const fromSource = process.argv.includes("--source");

const say = (line) => console.error(`viralcraft: ${line}`);

const wanted = `viralcraft ${pkg.version}`;

function works(file) {
  const run = spawnSync(file, ["--version"], { encoding: "utf8" });
  return run.status === 0 && run.stdout.trim() === wanted;
}

function releaseUrl() {
  const repo = (pkg.repository?.url ?? pkg.repository ?? "")
    .replace(/^git\+/, "")
    .replace(/\.git$/, "")
    .replace(/\/$/, "");
  return `${repo}/releases/download/v${pkg.version}/viralcraft-${target}.gz`;
}

// Writes beside the binary and renames, never overwriting it in place: macOS
// kills a signed binary whose file changed under its cached signature.
function place(write, what) {
  mkdirSync(nativeDir, { recursive: true });
  // Keep the .exe suffix so Windows will run the file for the version check.
  const tmp = join(nativeDir, win ? "viralcraft.partial.exe" : "viralcraft.partial");
  write(tmp);
  chmodSync(tmp, 0o755);
  if (!works(tmp)) {
    rmSync(tmp, { force: true });
    say(`the ${what} binary did not report ${wanted}.`);
    return false;
  }
  renameSync(tmp, binary);
  return true;
}

async function download() {
  if (!released.includes(target)) {
    say(`no prebuilt binary is made for ${target}.`);
    return false;
  }
  const url = releaseUrl();
  say(`downloading ${url}`);
  let body;
  try {
    const res = await fetch(url);
    if (!res.ok) {
      say(`download failed: HTTP ${res.status} for ${url}`);
      return false;
    }
    body = gunzipSync(Buffer.from(await res.arrayBuffer()));
  } catch (err) {
    say(`download failed: ${err.cause?.message ?? err.message}`);
    return false;
  }
  if (!place((tmp) => writeFileSync(tmp, body), "downloaded")) return false;
  say(`downloaded ${url.split("/").pop()} into bin/native/`);
  return true;
}

function findCargo() {
  if (spawnSync("cargo", ["--version"]).status === 0) return "cargo";
  // rustup's default bin dir is often missing from PATH in non-login shells.
  const cargoHome = process.env.CARGO_HOME ?? join(homedir(), ".cargo");
  const fallback = join(cargoHome, "bin", win ? "cargo.exe" : "cargo");
  return existsSync(fallback) ? fallback : null;
}

function buildFromSource() {
  const cargo = findCargo();
  if (!cargo) return false;
  say("building from source with cargo (the first build takes a few minutes)...");
  const build = spawnSync(cargo, ["build", "--release", "--locked", "-p", "viralcraft"], {
    cwd: root,
    stdio: "inherit",
  });
  if (build.status !== 0) {
    say("cargo build failed; see the output above.");
    return false;
  }
  const built = join(resolve(root, process.env.CARGO_TARGET_DIR ?? "target"), "release", exe);
  if (!place((tmp) => copyFileSync(built, tmp), "built")) return false;
  say("built from source into bin/native/");
  return true;
}

async function installBinary() {
  if (!fromSource && existsSync(binary) && works(binary)) {
    say(`${wanted} is already present.`);
    return;
  }
  if (!fromSource && (await download())) return;
  if (buildFromSource()) return;
  say(
    [
      fromSource
        ? "cannot build from source: cargo was not found or the build failed."
        : `there is no prebuilt binary for ${target} (version ${pkg.version}) yet, and Rust is not installed.`,
      "Two ways out:",
      "  1. Install Rust from https://rustup.rs, then run `npm install` again.",
      "  2. Wait for a release that includes your platform, then run `npm install` again.",
    ].join("\n"),
  );
  process.exit(1);
}

function linkSkills() {
  const skillsDir = join(root, ".agents", "skills");
  for (const name of ["viralcraft", "viralcraft-filmcraft"]) {
    const source = join(root, ".claude", "skills", name);
    const link = join(skillsDir, name);
    if (!existsSync(source)) {
      say(`skipping the Codex link for ${name}: .claude/skills/${name} is missing.`);
      continue;
    }
    let stat = null;
    try {
      stat = lstatSync(link);
    } catch {}
    if (stat?.isSymbolicLink()) {
      const current = readlinkSync(link).replace(/^\\\\\?\\/, "");
      if (resolve(skillsDir, current) === source) continue;
      rmSync(link);
    } else if (stat) {
      say(`leaving .agents/skills/${name} alone: it is a real folder, not a link.`);
      continue;
    }
    mkdirSync(skillsDir, { recursive: true });
    // Junctions need no admin rights or developer mode on Windows, but must be absolute.
    if (win) symlinkSync(source, link, "junction");
    else symlinkSync(join("..", "..", ".claude", "skills", name), link);
  }
}

await installBinary();
linkSkills();
for (const dir of ["inbox", "assets"]) mkdirSync(join(root, dir), { recursive: true });

console.error(`
ViralCraft is installed. Next:
  1. npm run setup   installs ffmpeg and the models.
  2. Put a viral video in inbox/ and your clips in assets/.
  3. Open Claude Code or Codex in this folder and say "copy this video with my clips".
`);
