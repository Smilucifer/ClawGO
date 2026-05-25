import { buildDoctorReport } from "$lib/utils/doctor";
import { runDiagnostics } from "$lib/api";
import { dbg, dbgWarn } from "$lib/utils/debug";
import type { DiagnosticsReport } from "$lib/types";

export class DoctorStore {
  loading = $state(false);
  error = $state<string | null>(null);
  report = $state<string | null>(null);
  rawReport = $state<DiagnosticsReport | null>(null);
  lastCwd = $state<string>("");

  async run(cwd: string, mcpServers?: import("$lib/types").McpServerInfo[]): Promise<void> {
    this.loading = true;
    this.error = null;
    this.lastCwd = cwd;
    try {
      dbg("doctor-store", "run", { cwd });
      this.rawReport = await runDiagnostics(cwd);
      this.report = await buildDoctorReport(cwd, mcpServers);
    } catch (e) {
      this.error = String(e);
      this.report = null;
      this.rawReport = null;
      dbgWarn("doctor-store", "run error", e);
    } finally {
      this.loading = false;
    }
  }

  clear(): void {
    this.report = null;
    this.rawReport = null;
    this.error = null;
  }
}
