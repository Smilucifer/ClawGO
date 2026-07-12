/**
 * File conversion module for office documents (docx, xlsx).
 *
 * Converts office files to markdown text for injection into the chat
 * as pastedBlocks. Zero backend changes — conversion happens entirely
 * in the frontend.
 */

import mammoth from "mammoth";
import TurndownService from "turndown";
import { interopDefault } from "./interop-default";

/** Maximum characters in converted output. Prevents context explosion from huge spreadsheets. */
export const MAX_CONVERTED_CHARS = 200_000;

/**
 * Convert a File (docx or xlsx) to markdown text.
 * @returns `{ text, format }` where format is always "markdown"
 * @throws User-friendly error message on failure
 */
export async function convertFile(file: File): Promise<{ text: string; format: string }> {
  const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
  const arrayBuffer = await file.arrayBuffer();

  let text: string;
  if (ext === "docx") {
    text = await convertDocx(arrayBuffer);
  } else if (ext === "xlsx") {
    text = await convertXlsx(arrayBuffer);
  } else {
    throw new Error(`Unsupported conversion format: .${ext}`);
  }

  // Truncate if too large
  if (text.length > MAX_CONVERTED_CHARS) {
    text =
      text.slice(0, MAX_CONVERTED_CHARS) +
      `\n\n[Truncated: original was ${text.length} characters, showing first ${MAX_CONVERTED_CHARS}]`;
  }

  return { text, format: "markdown" };
}

/** Convert a docx ArrayBuffer to markdown via mammoth → turndown. */
async function convertDocx(arrayBuffer: ArrayBuffer): Promise<string> {
  try {
    const result = await mammoth.convertToHtml({ arrayBuffer });
    const html = result.value;
    if (!html || html.trim().length === 0) {
      throw new Error("Document appears to be empty");
    }
    const td = new TurndownService({ headingStyle: "atx" });
    return td.turndown(html);
  } catch (e) {
    if (e instanceof Error && e.message === "Document appears to be empty") throw e;
    throw new Error(`Failed to read Word document: ${e instanceof Error ? e.message : String(e)}`);
  }
}

/** Convert an xlsx ArrayBuffer to markdown tables (one section per sheet). */
async function convertXlsx(arrayBuffer: ArrayBuffer): Promise<string> {
  try {
    // Dynamic import — shares the same chunk as OfficePreview's xlsx usage
    const XLSX = interopDefault(await import("xlsx"));
    const workbook = XLSX.read(arrayBuffer, { type: "array" });

    const sections: string[] = [];

    for (const sheetName of workbook.SheetNames) {
      const sheet = workbook.Sheets[sheetName];
      // header: 1 → array of arrays (each row = string[])
      const rows: string[][] = XLSX.utils.sheet_to_json(sheet, {
        header: 1,
        defval: "",
      });

      if (rows.length === 0) continue;

      // Normalize column count (pad shorter rows)
      const maxCols = rows.reduce((m, r) => Math.max(m, r.length), 0);
      const normalized = rows.map((r) => {
        while (r.length < maxCols) r.push("");
        return r.map((c) => String(c));
      });

      // Build markdown table
      const header = "| " + normalized[0].map((c) => c.replace(/\|/g, "\\|")).join(" | ") + " |";
      const separator = "| " + normalized[0].map(() => "---").join(" | ") + " |";
      const body = normalized
        .slice(1)
        .map((row) => "| " + row.map((c) => c.replace(/\|/g, "\\|")).join(" | ") + " |")
        .join("\n");

      const table = [header, separator, body].filter(Boolean).join("\n");
      sections.push(`## Sheet: ${sheetName}\n\n${table}`);
    }

    if (sections.length === 0) {
      throw new Error("Spreadsheet appears to be empty");
    }

    return sections.join("\n\n");
  } catch (e) {
    if (
      e instanceof Error &&
      (e.message === "Spreadsheet appears to be empty" ||
        e.message === "Document appears to be empty")
    )
      throw e;
    throw new Error(`Failed to read spreadsheet: ${e instanceof Error ? e.message : String(e)}`);
  }
}
