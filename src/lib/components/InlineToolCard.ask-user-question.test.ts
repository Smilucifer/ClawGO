// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import InlineToolCard from "./InlineToolCard.svelte";
import type { BusToolItem } from "$lib/types";

function makeAskTool(): BusToolItem {
  return {
    tool_use_id: "ask-multi",
    tool_name: "AskUserQuestion",
    status: "permission_prompt",
    permission_request_id: "req-ask-multi",
    input: {
      questions: [
        {
          header: "审查方1",
          question: "选择参与方案讨论的 provider（可多选，第 1 组）？",
          multiSelect: true,
          options: [{ label: "Claude" }, { label: "DeepSeek" }, { label: "GLM" }],
        },
        {
          header: "审查方2",
          question: "选择参与方案讨论的 provider（可多选，第 2 组）？",
          multiSelect: true,
          options: [{ label: "QWEN" }, { label: "KIMI" }, { label: "Packy" }],
        },
      ],
    },
  };
}

describe("InlineToolCard AskUserQuestion multi-question multi-select", () => {
  it("submits multiple selected options for each multi-select question", async () => {
    const onPermissionRespond = vi.fn();

    render(InlineToolCard, {
      props: {
        tool: makeAskTool(),
        onPermissionRespond,
      },
    });

    await fireEvent.click(screen.getByRole("button", { name: /Claude/ }));
    await fireEvent.click(screen.getByRole("button", { name: /DeepSeek/ }));
    await fireEvent.click(screen.getByRole("button", { name: /QWEN/ }));
    await fireEvent.click(screen.getByRole("button", { name: /KIMI/ }));
    await fireEvent.click(screen.getByRole("button", { name: /提交（2\/2）/ }));

    expect(onPermissionRespond).toHaveBeenCalledTimes(1);
    expect(onPermissionRespond).toHaveBeenCalledWith(
      "req-ask-multi",
      "allow",
      undefined,
      expect.objectContaining({
        answers: {
          "选择参与方案讨论的 provider（可多选，第 1 组）？": "Claude, DeepSeek",
          "选择参与方案讨论的 provider（可多选，第 2 组）？": "QWEN, KIMI",
        },
      }),
    );
  });
});
