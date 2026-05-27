/**
 * Matches file paths in text.
 * Detects: Windows paths (C:\), Unix paths (/...), relative paths (./, ../, dir/)
 * Excludes URLs (http://, https://) and path segments inside URLs (e.g. /docs/readme.md in https://example.com/docs/readme.md).
 */
export const FILE_PATH_PATTERN =
  /((?!https?:\/\/)(?<!\/)(?:[A-Za-z]:[\\/]|\.\.?[\\/]|\/[^/\s]|\b[\w-]+[\\/])[^\s]*\.[a-zA-Z0-9]{1,10}\b)/g;

export function hasFilePath(text: string): boolean {
  const re = new RegExp(FILE_PATH_PATTERN);
  return re.test(text);
}
