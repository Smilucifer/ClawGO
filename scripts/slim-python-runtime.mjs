#!/usr/bin/env node
/**
 * Slim down the embedded python-runtime before Tauri bundles it.
 *
 * python-runtime/python/ is git-ignored (see .gitignore), so everything removed
 * here is untracked build input — deleting it never dirties the working tree.
 * Tauri bundles the whole tree via `python-runtime/python/**` globs, so trimming
 * dead weight (debug symbols, test suites, bytecode caches, pip, Tk/Tcl GUI libs)
 * directly shrinks the NSIS/MSI installer.
 *
 * Nothing removed here is used at runtime by our scripts (verified: no matplotlib,
 * no tkinter imports; scripts use requests/akshare/xtquant/scrapling/yfinance).
 * Browser drivers (playwright/patchright) and xtquant/py_mini_racer are LEFT
 * INTACT by design — see docs and the size-optimization decision record.
 */
import { readdirSync, rmSync, statSync, existsSync } from "fs";
import { join } from "path";

// beforeBuildCommand runs with cwd = src-tauri/, but allow project-root runs too.
const PY_DIR = existsSync("python-runtime/python")
  ? "python-runtime/python"
  : "src-tauri/python-runtime/python";

if (!existsSync(PY_DIR)) {
  console.log("  python-runtime/python missing — nothing to slim");
  process.exit(0);
}

const DRY_RUN = process.argv.includes("--dry-run");

let freedBytes = 0;
let removedCount = 0;

function dirSize(path) {
  let total = 0;
  let entries;
  try {
    entries = readdirSync(path, { withFileTypes: true });
  } catch {
    return 0;
  }
  for (const e of entries) {
    const full = join(path, e.name);
    try {
      if (e.isDirectory()) total += dirSize(full);
      else total += statSync(full).size;
    } catch {
      /* vanished mid-walk — ignore */
    }
  }
  return total;
}

function remove(full) {
  try {
    const size = statSync(full).isDirectory() ? dirSize(full) : statSync(full).size;
    if (DRY_RUN) console.log(`  [dry-run] would remove ${full} (${(size / 1024 / 1024).toFixed(1)} MB)`);
    else rmSync(full, { recursive: true, force: true });
    freedBytes += size;
    removedCount++;
  } catch (e) {
    console.warn(`  ! could not remove ${full}: ${e.message}`);
  }
}

// Recursively walk PY_DIR, calling onMatch(fullPath, dirent) for each entry.
// Returning true from onMatch means "removed — do not descend".
function walk(dir, onMatch) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const e of entries) {
    const full = join(dir, e.name);
    if (onMatch(full, e)) continue;
    if (e.isDirectory()) walk(full, onMatch);
  }
}

// 1. Debug symbols (.pdb) — never needed at runtime (~85 MB raw).
// 2. Bundled test suites — dirs named exactly `test`/`tests` (~53 MB raw).
//    Package API dirs like numpy/testing are preserved (name != test/tests).
// 3. Bytecode caches — __pycache__ rebuilds lazily on first run (~37 MB raw).
walk(PY_DIR, (full, e) => {
  if (e.isFile() && e.name.toLowerCase().endsWith(".pdb")) {
    remove(full);
    return true;
  }
  if (e.isDirectory() && (e.name === "test" || e.name === "tests" || e.name === "__pycache__")) {
    remove(full);
    return true;
  }
  return false;
});

// 4. pip + ensurepip — an embedded runtime never self-installs packages (~14 MB).
// 5. Tk/Tcl GUI stack — headless data scripts never import tkinter (~12 MB).
const EXTRA_PATHS = [
  "Lib/site-packages/pip",
  "Lib/ensurepip",
  "Lib/tkinter",
  "Lib/idlelib",
  "tcl",
  "DLLs/tcl86t.dll",
  "DLLs/tk86t.dll",
  "DLLs/_tkinter.pyd",
];
for (const rel of EXTRA_PATHS) {
  const full = join(PY_DIR, rel);
  if (existsSync(full)) remove(full);
}

const freedMb = (freedBytes / (1024 * 1024)).toFixed(1);
console.log(`  ✓ slimmed python-runtime: removed ${removedCount} items, freed ${freedMb} MB`);
