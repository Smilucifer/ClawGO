type PipeExecRunLike = {
  id: string;
  status?: string;
  execution_path?: string;
};

const PIPE_EXEC_REPLAY_STATUSES = new Set(["completed", "failed", "stopped"]);

export function getPipeExecTerminalReplayKey(
  run: PipeExecRunLike | null | undefined,
  useStreamSession: boolean,
  hasTerminal: boolean,
): string {
  if (!run || useStreamSession || !hasTerminal) return "";
  if (run.execution_path !== "pipe_exec") return "";
  if (!run.status || !PIPE_EXEC_REPLAY_STATUSES.has(run.status)) return "";
  return `${run.id}:${run.status}`;
}
