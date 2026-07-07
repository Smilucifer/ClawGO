#!/usr/bin/env node
/**
 * Remove transient runtime/debug probe files from python-runtime/scripts before
 * bundling. The app (and dev iterations) write throwaway `_xq_*probe*.py` scripts
 * and `*.err` / `*_out.json` outputs into that directory. Tauri bundles the whole
 * folder via a `scripts/**` glob, so a probe file that vanishes between WiX's
 * candle (manifest) and light (link) steps aborts the MSI build with LGHT0103.
 *
 * Only untracked throwaway files are removed; the 12 git-tracked scripts stay.
 */
import { readdirSync, rmSync, statSync, existsSync } from "fs";
import { join } from "path";

// beforeBuildCommand runs with cwd = src-tauri/, but the script may also be run
// from the project root. Resolve whichever layout exists.
const SCRIPTS_DIR = existsSync("python-runtime/scripts")
  ? "python-runtime/scripts"
  : "src-tauri/python-runtime/scripts";

// Throwaway artifacts that must never enter the bundle.
const JUNK_PATTERNS = [
  /^_.*probe.*\.py$/i, // _xq_scrapling_probe*.py, _xq_cookie_probe.py, …
  /\.err$/i, // em_market.err
  /_out\.json$/i, // em_market_out.json
];

let removed = 0;
let entries;
try {
  entries = readdirSync(SCRIPTS_DIR);
} catch {
  // Directory missing (e.g. CI checkout without python-runtime) — nothing to do.
  process.exit(0);
}

for (const name of entries) {
  if (!JUNK_PATTERNS.some((re) => re.test(name))) continue;
  const full = join(SCRIPTS_DIR, name);
  try {
    if (statSync(full).isFile()) {
      rmSync(full);
      console.log(`  ✓ removed runtime probe: ${name}`);
      removed++;
    }
  } catch (e) {
    console.warn(`  ! could not remove ${name}: ${e.message}`);
  }
}

if (removed === 0) {
  console.log("  scripts/ clean — no runtime probes to remove");
}
