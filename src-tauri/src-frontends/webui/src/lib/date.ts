const DATE_ONLY_PATTERN = /^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$/;
const COMPACT_DATE_PATTERN = /^(\d{4})(\d{2})(\d{2})$/;

function createLocalDate(year: number, month: number, day: number): Date | null {
  const date = new Date(year, month - 1, day);
  if (
    date.getFullYear() !== year ||
    date.getMonth() !== month - 1 ||
    date.getDate() !== day
  ) {
    return null;
  }
  return date;
}

/**
 * 将注册表日期或 RFC 3339 时间统一显示为 Windows 本地短日期。
 * 纯日期按日历日期解析，避免 `new Date("YYYY-MM-DD")` 被当作 UTC 导致日期偏移。
 */
export function formatWindowsDate(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  if (!trimmed) return null;

  const dateOnlyMatch = trimmed.match(DATE_ONLY_PATTERN);
  const compactDateMatch = trimmed.match(COMPACT_DATE_PATTERN);
  let date: Date | null = null;

  if (dateOnlyMatch) {
    date = createLocalDate(
      Number(dateOnlyMatch[1]),
      Number(dateOnlyMatch[2]),
      Number(dateOnlyMatch[3]),
    );
  } else if (compactDateMatch) {
    date = createLocalDate(
      Number(compactDateMatch[1]),
      Number(compactDateMatch[2]),
      Number(compactDateMatch[3]),
    );
  } else {
    const parsed = new Date(trimmed);
    if (!Number.isNaN(parsed.valueOf())) date = parsed;
  }

  return date
    ? new Intl.DateTimeFormat(undefined, {
        dateStyle: "short",
      }).format(date)
    : null;
}
