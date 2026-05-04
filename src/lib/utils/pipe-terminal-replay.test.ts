import { describe, expect, it } from "vitest";
import { getPipeExecTerminalReplayKey } from "./pipe-terminal-replay";

describe("getPipeExecTerminalReplayKey", () => {
  it("does not replay a live pipe-exec run", () => {
    expect(
      getPipeExecTerminalReplayKey(
        { id: "run-1", execution_path: "pipe_exec", status: "running" },
        false,
        true,
      ),
    ).toBe("");
  });

  it("replays only terminal pipe-exec runs after the terminal is ready", () => {
    expect(
      getPipeExecTerminalReplayKey(
        { id: "run-1", execution_path: "pipe_exec", status: "completed" },
        false,
        true,
      ),
    ).toBe("run-1:completed");
  });
});
