import { MAX_PASTE_BLOCKS } from "$lib/utils/file-types";
import { uuid } from "$lib/utils/uuid";

export interface PastedBlock {
  id: string;
  text: string;
  lineCount: number;
  charCount: number;
  preview: string;
  ext?: string;
}

interface RichPasteResult {
  preventDefault: boolean;
  pastedBlocks: PastedBlock[];
}

export function handleRichPaste(
  e: ClipboardEvent,
  currentBlockCount: number,
): RichPasteResult {
  const result: RichPasteResult = {
    preventDefault: false,
    pastedBlocks: [],
  };

  const text = e.clipboardData?.getData("text/plain");
  if (!text) return result;

  const lines = text.split("\n");
  const lineCount = lines.length;
  const charCount = text.length;

  if (lineCount < 5 && charCount < 500) return result;

  if (currentBlockCount >= MAX_PASTE_BLOCKS) {
    result.preventDefault = true;
    return result;
  }

  result.preventDefault = true;
  const firstLine = lines[0].trim();
  const preview = firstLine.length > 40 ? firstLine.slice(0, 40) + "..." : firstLine;
  result.pastedBlocks.push({
    id: uuid().slice(0, 8),
    text,
    lineCount,
    charCount,
    preview,
  });

  return result;
}

export function composeRichMessageText(
  typed: string,
  blocks: PastedBlock[],
): string {
  const parts: string[] = blocks.map((b) => b.text);
  const trimmed = typed.trim();
  if (trimmed) parts.push(trimmed);
  return parts.join("\n\n");
}
