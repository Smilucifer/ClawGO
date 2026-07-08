#!/usr/bin/env node
// Run Rust tests on Windows, embedding a Common-Controls v6 manifest into each
// test binary first.
//
// Why: tauri-plugin-dialog statically imports comctl32.dll!TaskDialogIndirect,
// which exists only in Common-Controls v6 (WinSxS). System32\comctl32.dll is
// v5.82 and lacks that export. The main ClawGO.exe gets a CC6 manifest from
// tauri-build, but `cargo test` binaries do not, so without it every test binary
// dies at load with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139) before main runs.
// Cargo has no link-arg scope that reaches lib unit tests without also hitting the
// bin (which collides with tauri-build's own manifest), so we embed post-compile
// with mt.exe. See CLAUDE.md §11.
//
// Flow:
//   1. `cargo test --no-run --message-format=json` → collect test binary paths.
//   2. mt.exe embeds the CC6 manifest into each (idempotent).
//   3. `cargo test` runs them (no relink since sources are unchanged).
//
// Any extra CLI args are forwarded to both cargo invocations, e.g.
//   node scripts/rust-test.mjs --lib storage::memos
//   node scripts/rust-test.mjs -- --nocapture

import { spawnSync, execFileSync } from 'node:child_process';
import { existsSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(scriptDir, '..');
const manifestPath = join(scriptDir, 'common-controls-v6.manifest');
const cargoManifest = join(repoRoot, 'src-tauri', 'Cargo.toml');

if (process.platform !== 'win32') {
  // Non-Windows: the loader issue does not exist; run cargo test directly.
  const r = spawnSync('cargo', ['test', '--manifest-path', cargoManifest, ...process.argv.slice(2)], {
    stdio: 'inherit',
  });
  process.exit(r.status ?? 1);
}

function fail(msg) {
  console.error(`[rust-test] ${msg}`);
  process.exit(1);
}

if (!existsSync(manifestPath)) fail(`manifest not found: ${manifestPath}`);

// Locate mt.exe from the Windows 10/11 SDK (newest version wins).
function findMt() {
  const bases = [
    'C:/Program Files (x86)/Windows Kits/10/bin',
    'C:/Program Files/Windows Kits/10/bin',
  ];
  for (const base of bases) {
    if (!existsSync(base)) continue;
    const versioned = readdirSync(base)
      .filter((d) => /^10\./.test(d))
      .sort()
      .reverse()
      .map((d) => join(base, d, 'x64', 'mt.exe'));
    for (const c of [...versioned, join(base, 'x64', 'mt.exe')]) {
      if (existsSync(c)) return c;
    }
  }
  return null;
}

const mt = findMt();
if (!mt) fail('mt.exe not found in Windows SDK (needed to embed the manifest).');

const forwarded = process.argv.slice(2);

// Step 1: compile test binaries and collect their paths.
console.error('[rust-test] compiling test binaries (cargo test --no-run)...');
const compile = spawnSync(
  'cargo',
  ['test', '--manifest-path', cargoManifest, '--no-run', '--message-format=json', ...forwarded],
  { encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 },
);
if (compile.status !== 0) {
  process.stderr.write(compile.stderr ?? '');
  fail('compilation failed.');
}

const binaries = [];
for (const line of compile.stdout.split(/\r?\n/)) {
  if (!line.startsWith('{')) continue;
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    continue;
  }
  if (msg.reason === 'compiler-artifact' && msg.profile?.test && msg.executable) {
    binaries.push(msg.executable);
  }
}

if (binaries.length === 0) fail('no test binaries produced.');

// Step 2: embed the CC6 manifest into each test binary (idempotent).
for (const bin of binaries) {
  try {
    execFileSync(mt, ['-nologo', '-manifest', manifestPath, `-outputresource:${bin};#1`], {
      stdio: 'pipe',
    });
    console.error(`[rust-test] embedded manifest → ${bin}`);
  } catch (e) {
    fail(`mt.exe failed on ${bin}: ${e.stderr?.toString() ?? e.message}`);
  }
}

// Step 3: run the tests (sources unchanged → no relink → manifest survives).
console.error('[rust-test] running tests...');
const run = spawnSync('cargo', ['test', '--manifest-path', cargoManifest, ...forwarded], {
  stdio: 'inherit',
});
process.exit(run.status ?? 1);
