import { formatNumber, NONE } from "../format";

/** A column of a `UiTable`. `key` names the row's field and the `#cell-<key>` and `#head-<key>` slots. */
export interface TableColumn {
  key: string;
  label: string;
  /** Figures (`fig`) default to the end edge, everything else to the start. */
  align?: "start" | "end" | "center";
  /** Any CSS width; the other columns share what is left. */
  width?: string;
  /** Mono and tabular, for numbers. */
  fig?: boolean;
  sort?: boolean;
  /** Hide the column when the table's own width is under this breakpoint (`sm` 30rem, `md` 48rem). */
  hide?: "sm" | "md";
}

export type SortDir = "asc" | "desc";
export interface SortState {
  key: string;
  dir: SortDir;
}

/** Where a column's text sits: its own choice, else the end edge for figures and the start edge for words. */
export const columnAlign = (c: TableColumn): "start" | "end" | "center" => c.align ?? (c.fig ? "end" : "start");

/** The field of a row a column shows. */
export const cellValue = (row: object, key: string): unknown => (row as Record<string, unknown>)[key];

/** A value as plain cell text: figures grouped, nothing as an en dash. */
export function cellText(value: unknown): string {
  if (value == null || value === "") return NONE;
  if (typeof value === "number") return formatNumber(value, Number.isInteger(value) ? 0 : 1);
  return String(value);
}

/** Orders two cells: numbers by size, text by locale (digits in order), nothing last whichever way it runs. */
export function compareValues(a: unknown, b: unknown, dir: SortDir): number {
  const none = (v: unknown) => v == null || v === "";
  if (none(a) || none(b)) return none(a) === none(b) ? 0 : none(a) ? 1 : -1;
  const order =
    typeof a === "number" && typeof b === "number"
      ? a - b
      : String(a).localeCompare(String(b), "en", { numeric: true, sensitivity: "base" });
  return dir === "asc" ? order : -order;
}

/** A sorted copy of the rows; the same order (a stable sort) when nothing is chosen. */
export function sortRows<T extends object>(rows: readonly T[], sort: SortState | null | undefined): T[] {
  if (!sort) return [...rows];
  return [...rows].sort((a, b) => compareValues(cellValue(a, sort.key), cellValue(b, sort.key), sort.dir));
}

/** What a header click does: figures start with the biggest, words with the first; then it flips; it never clears. */
export function nextSort(current: SortState | null | undefined, column: TableColumn): SortState {
  if (current?.key === column.key) return { key: column.key, dir: current.dir === "asc" ? "desc" : "asc" };
  return { key: column.key, dir: column.fig ? "desc" : "asc" };
}
