<script setup lang="ts" generic="T extends object">
import { PhCaretDown, PhCaretUp } from "@phosphor-icons/vue";
import { computed, getCurrentInstance, onBeforeUnmount, onMounted, ref, useTemplateRef } from "vue";
import {
  cellText,
  cellValue,
  columnAlign,
  nextSort,
  sortRows,
  type SortDir,
  type SortState,
  type TableColumn,
} from "./table";
import UiEmpty from "./UiEmpty.vue";

/**
 * The one table. Rows are 28 px with hairlines between and nothing at the sides; the head is quiet, sentence case,
 * 24 px. Figures are mono and sit at the end edge; the first column is the row's name. A table wider than its
 * container scrolls inside itself with a fade at the edge that has more. Write a column's cell with
 * `#cell-<key>="{ row, value }"` and its head with `#head-<key>`; `#row-aside="{ row }"` is a line under a row listed
 * in `expanded`; `#foot` is a last row. Give it a `@select` and its rows are buttons. Sorting is done here unless
 * `manual` (then the parent sorts on `@sort`). `sticky` pins the head to the pane that scrolls the page, so the table
 * itself does not scroll sideways then: hide columns with `hide` instead.
 */
const props = withDefaults(
  defineProps<{
    columns: TableColumn[];
    rows: readonly T[];
    rowKey: keyof T | ((row: T) => string);
    dense?: boolean;
    /** The key (see `rowKey`) of the chosen row. */
    selected?: string;
    sticky?: boolean;
    empty?: string;
    loading?: boolean;
    /** The table's name for a screen reader. */
    label?: string;
    /** The order is the parent's: the table shows the arrow and emits `sort`, and shows rows as given. */
    manual?: boolean;
    sort?: SortState | null;
    /** Row keys whose `#row-aside` is shown. */
    expanded?: string[];
    /** Pads the first and last cell more, for a table that touches its container's edge. */
    edge?: boolean;
  }>(),
  { dense: true },
);

const emit = defineEmits<{ select: [row: T]; sort: [key: string, dir: SortDir] }>();

/** A `@select` makes a row a button. */
const interactive = !!getCurrentInstance()?.vnode.props?.["onSelect"];

const keyOf = (row: T): string =>
  typeof props.rowKey === "function" ? props.rowKey(row) : String(row[props.rowKey as keyof T]);

const own = ref<SortState | null>(null);
const sorting = computed(() => (props.sort !== undefined ? props.sort : own.value));
const shown = computed(() => (props.manual ? [...props.rows] : sortRows(props.rows, sorting.value)));

function toggleSort(c: TableColumn) {
  const next = nextSort(sorting.value, c);
  own.value = next;
  emit("sort", next.key, next.dir);
}

const ariaSort = (c: TableColumn) =>
  c.sort
    ? sorting.value?.key === c.key
      ? sorting.value.dir === "asc"
        ? "ascending"
        : "descending"
      : "none"
    : undefined;

// A wide table scrolls inside its own frame, with a fade where there is more to see.
const wrap = useTemplateRef<HTMLElement>("wrap");
const more = ref({ start: false, end: false });
function measure() {
  const el = wrap.value;
  if (!el) return;
  const max = el.scrollWidth - el.clientWidth;
  const at = Math.abs(el.scrollLeft);
  more.value = { start: max > 1 && at > 1, end: max > 1 && at < max - 1 };
}
let observer: ResizeObserver | undefined;
onMounted(() => {
  measure();
  if (typeof ResizeObserver === "function" && wrap.value) {
    observer = new ResizeObserver(measure);
    observer.observe(wrap.value);
    const table = wrap.value.firstElementChild;
    if (table) observer.observe(table);
  }
});
onBeforeUnmount(() => observer?.disconnect());
const scrolls = computed(() => more.value.start || more.value.end);

function onRowKey(e: KeyboardEvent, row: T) {
  if (e.target !== e.currentTarget) return;
  if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
    emit("select", row);
  }
}
</script>

<template>
  <div
    ref="wrap"
    class="ui-table-wrap"
    :class="{ 'is-sticky': sticky, 'is-scroll': scrolls, 'more-start': more.start, 'more-end': more.end }"
    :role="scrolls ? 'region' : undefined"
    :aria-label="scrolls ? label : undefined"
    :tabindex="scrolls ? 0 : undefined"
    @scroll.passive="measure"
  >
    <table
      class="ui-table"
      :class="{ 'is-dense': dense, 'is-edge': edge, 'is-interactive': interactive }"
      :aria-label="label"
      :aria-busy="loading || undefined"
    >
      <thead>
        <tr>
          <th
            v-for="c in columns"
            :key="c.key"
            scope="col"
            :class="[`is-${columnAlign(c)}`, { 'is-fig': c.fig }]"
            :data-hide="c.hide"
            :aria-sort="ariaSort(c)"
            :style="c.width ? { width: c.width } : undefined"
          >
            <button v-if="c.sort" type="button" class="ui-table-sort" @click="toggleSort(c)">
              <slot :name="`head-${c.key}`">{{ c.label }}</slot>
              <PhCaretUp
                v-if="sorting?.key === c.key && sorting.dir === 'asc'"
                class="ui-table-arrow"
                weight="bold"
                aria-hidden="true"
              />
              <PhCaretDown v-else-if="sorting?.key === c.key" class="ui-table-arrow" weight="bold" aria-hidden="true" />
            </button>
            <slot v-else :name="`head-${c.key}`">{{ c.label }}</slot>
          </th>
        </tr>
      </thead>
      <tbody>
        <template v-if="loading">
          <tr v-for="n in 5" :key="n" class="ui-table-skeleton" aria-hidden="true">
            <td v-for="c in columns" :key="c.key" :data-hide="c.hide"><span class="skel" /></td>
          </tr>
        </template>
        <tr v-else-if="!shown.length" class="ui-table-none">
          <td :colspan="columns.length">
            <UiEmpty dense :title="empty ?? 'Nothing to show.'" />
          </td>
        </tr>
        <template v-else>
          <template v-for="row in shown" :key="keyOf(row)">
            <tr
              :class="{ 'is-selected': selected === keyOf(row) }"
              :aria-current="selected === keyOf(row) ? 'true' : undefined"
              :tabindex="interactive ? 0 : undefined"
              @click="interactive && emit('select', row)"
              @keydown="interactive && onRowKey($event, row)"
            >
              <td
                v-for="(c, i) in columns"
                :key="c.key"
                :class="[`is-${columnAlign(c)}`, { 'is-fig': c.fig, 'is-first': i === 0 }]"
                :data-hide="c.hide"
              >
                <slot :name="`cell-${c.key}`" :row="row" :value="cellValue(row, c.key)">
                  {{ cellText(cellValue(row, c.key)) }}
                </slot>
              </td>
            </tr>
            <tr v-if="$slots['row-aside'] && expanded?.includes(keyOf(row))" class="ui-table-aside">
              <td :colspan="columns.length"><slot name="row-aside" :row="row" /></td>
            </tr>
          </template>
        </template>
      </tbody>
      <tfoot v-if="$slots['foot']">
        <tr>
          <td :colspan="columns.length"><slot name="foot" /></td>
        </tr>
      </tfoot>
    </table>
  </div>
</template>

<style scoped>
.ui-table-wrap {
  --_fade-start: 0px;
  --_fade-end: 0px;

  container: ui-table / inline-size;
  min-width: 0;
  max-width: 100%;
  overflow-x: auto;
  mask-image: linear-gradient(
    to right,
    transparent 0,
    black var(--_fade-start),
    black calc(100% - var(--_fade-end)),
    transparent 100%
  );
}

.ui-table-wrap.more-start {
  --_fade-start: var(--sp-5);
}

.ui-table-wrap.more-end {
  --_fade-end: var(--sp-5);
}

.ui-table-wrap.is-scroll {
  border: var(--e-stroke-hair) solid var(--c-line-strong);
  border-radius: var(--r-1);
}

.ui-table-wrap:focus-visible {
  outline: var(--focus-ring);
}

/* A pinned head sticks to the pane that scrolls, so this frame must not be a scroller itself. */
.ui-table-wrap.is-sticky {
  overflow: visible;
}

.ui-table {
  width: 100%;
  border-collapse: collapse;
  font: var(--t-body);
}

th,
td {
  box-sizing: border-box;
  height: var(--h-row);
  padding: var(--sp-1) var(--sp-2);
  text-align: start;
  vertical-align: middle;
}

.is-dense td {
  height: var(--h-table-row);
}

th {
  height: var(--h-table-head);
  background: var(--c-raise);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
  color: var(--c-ink-2);
  font: 500 var(--e-text-xs) / 1.35 var(--e-font-sans);
  white-space: nowrap;
}

.is-sticky th {
  position: sticky;
  inset-block-start: 0;
  z-index: var(--e-z-sticky);
}

.is-end {
  text-align: end;
}

.is-center {
  text-align: center;
}

/* Figures take what they need and no more; words take the rest, and a table that cannot fit scrolls. */
th.is-fig,
td.is-fig {
  width: 1%;
}

th:first-child:not(.is-fig),
td.is-first:not(.is-fig) {
  min-width: 8rem;
}

td.is-fig {
  font: var(--t-fig);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

td.is-first {
  font: var(--t-body-strong);
}

td.is-first.is-fig {
  font: var(--t-fig);
  font-weight: 600;
}

tbody tr + tr > td,
tbody tr:not(.ui-table-aside) + .ui-table-aside > td {
  border-block-start: var(--e-stroke-hair) solid var(--c-line);
}

.is-edge th:first-child,
.is-edge td:first-child {
  padding-inline-start: var(--sp-3);
}

.is-edge th:last-child,
.is-edge td:last-child {
  padding-inline-end: var(--sp-3);
}

tbody tr:hover > td {
  background: var(--c-raise);
}

tr.is-selected > td {
  background: var(--c-sunk);
}

tr.is-selected > td:first-child {
  box-shadow: inset var(--e-stroke-focus) 0 0 var(--c-act);
}

.is-interactive tbody tr:not(.ui-table-none, .ui-table-aside, .ui-table-skeleton) {
  cursor: pointer;
}

.is-interactive tbody tr:focus-visible {
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

.ui-table-aside > td {
  padding-block: var(--sp-2);
  background: var(--c-raise);
}

.ui-table-none:hover > td {
  background: transparent;
}

.ui-table-skeleton .skel {
  height: 0.75em;
}

tfoot td {
  height: var(--h-table-row);
  border-block-start: var(--e-stroke-hair) solid var(--c-line);
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.ui-table-sort {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  min-height: var(--h-table-head);
  margin: calc(-1 * var(--sp-1));
  padding: 0 var(--sp-1);
  border: 0;
  border-radius: var(--r-1);
  background: transparent;
  color: inherit;
  font: inherit;
  cursor: pointer;
}

/* The head is short; with a finger its button reaches a little past it, so it is still a target. */
@media (pointer: coarse) {
  .ui-table-sort::after {
    content: "";
    position: absolute;
    inset: calc(-1 * var(--sp-2)) 0;
  }
}

.ui-table-sort:hover {
  color: var(--c-ink);
}

.ui-table-sort:focus-visible {
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

th[aria-sort="ascending"],
th[aria-sort="descending"] {
  color: var(--c-ink);
}

.ui-table-arrow {
  flex: none;
  font-size: var(--i-sm);
}

/* A column hides when the table itself is narrower than its breakpoint: sm 30rem, md 48rem. */
@container ui-table (width < 30rem) {
  [data-hide="sm"] {
    display: none;
  }
}

@container ui-table (width < 48rem) {
  [data-hide="md"] {
    display: none;
  }
}
</style>
