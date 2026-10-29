<script setup lang="ts">
import { PhCheckCircle, PhGameController, PhGear, PhTray } from "@phosphor-icons/vue";
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import { shown } from "../../scheme";
import UiSection from "../UiSection.vue";
import UiTable from "../UiTable.vue";
import { contrastRatio, resolveColor } from "./contrast";

/** The scales as they are drawn on this page: spacing, type, colour roles, radii, heights, icons, motion, layout. */

const spacing = ["--sp-1", "--sp-2", "--sp-3", "--sp-4", "--sp-5", "--sp-6"];
const type = [
  ["--t-title", "Sheet title, dialog title", "Sans 600 20/1.3"],
  ["--t-head", "Section title", "Sans 600 16/1.35"],
  ["--t-body", "Everything: panels, rows, forms, tables", "Sans 400 14/1.45"],
  ["--t-body-strong", "Row primary text, first column", "Sans 600 14/1.45"],
  ["--t-meta", "Labels, helper text, timestamps", "Sans 400 12/1.35"],
  ["--t-fig", "1,840  +0.6  e2e4", "Mono 500 14/1.3"],
  ["--t-fig-lg", "1,840", "Mono 600 20/1.2"],
  ["--t-fig-xl", "1,840", "Mono 600 28/1.1"],
  ["--t-clock", "3:58", "Mono 600 18/1"],
] as const;

const surfaces = ["--c-page", "--c-pane", "--c-raise", "--c-sunk"];
const inks = ["--c-ink", "--c-ink-2", "--c-ink-3"];
const lines = ["--c-line", "--c-line-strong"];
const roles = ["--c-act", "--c-act-wash", "--c-win", "--c-loss", "--c-draw", "--c-warn", "--c-info"];
const washes = ["--c-win-wash", "--c-loss-wash", "--c-warn-wash", "--c-info-wash", "--c-neutral-wash"];
const heights = [
  "--h-control",
  "--h-row",
  "--h-row-2",
  "--h-rail-row",
  "--h-table-row",
  "--h-table-head",
  "--h-move",
  "--h-tab",
  "--h-header",
  "--h-badge",
];
const widths = ["--w-rail", "--w-panel", "--w-sheet", "--w-sheet-wide"];

// Contrast of the text pairs that matter, measured on the page as it is now (the scheme switch above changes it).
interface Pair {
  id: string;
  text: string;
  on: string;
  need: number;
  note: string;
}
const pairs: Pair[] = [
  { id: "ink-pane", text: "--c-ink", on: "--c-pane", need: 4.5, note: "Body text" },
  { id: "ink-page", text: "--c-ink", on: "--c-page", need: 4.5, note: "Text on the stage" },
  { id: "ink2-pane", text: "--c-ink-2", on: "--c-pane", need: 4.5, note: "Labels, meta" },
  { id: "ink2-raise", text: "--c-ink-2", on: "--c-raise", need: 4.5, note: "Table head" },
  { id: "ink2-sunk", text: "--c-ink-2", on: "--c-sunk", need: 4.5, note: "Segmented option" },
  { id: "ink-sunk", text: "--c-ink", on: "--c-sunk", need: 4.5, note: "Selected row" },
  { id: "act-pane", text: "--c-act", on: "--c-pane", need: 4.5, note: "Act text (a count)" },
  { id: "actink-act", text: "--c-act-ink", on: "--c-act", need: 4.5, note: "Primary button" },
  { id: "win-pane", text: "--c-win", on: "--c-pane", need: 4.5, note: "Won" },
  { id: "loss-pane", text: "--c-loss", on: "--c-pane", need: 4.5, note: "Lost" },
  { id: "warn-pane", text: "--c-warn", on: "--c-pane", need: 4.5, note: "Warning" },
  { id: "info-pane", text: "--c-info", on: "--c-pane", need: 4.5, note: "Info" },
  { id: "win-wash", text: "--c-win", on: "--c-win-wash", need: 4.5, note: "Badge: win" },
  { id: "loss-wash", text: "--c-loss", on: "--c-loss-wash", need: 4.5, note: "Badge: loss" },
  { id: "warn-wash", text: "--c-warn", on: "--c-warn-wash", need: 4.5, note: "Badge: warn" },
  { id: "info-wash", text: "--c-info", on: "--c-info-wash", need: 4.5, note: "Badge: info" },
  { id: "act-wash", text: "--c-act", on: "--c-act-wash", need: 4.5, note: "Badge: act" },
  { id: "ink2-neutral", text: "--c-ink-2", on: "--c-neutral-wash", need: 4.5, note: "Badge: neutral" },
  { id: "line-pane", text: "--c-line-strong", on: "--c-pane", need: 3, note: "Control borders (graphics)" },
  { id: "ink3-pane", text: "--c-ink-3", on: "--c-pane", need: 0, note: "Disabled, ticks: never information" },
];

interface Measured extends Pair {
  ratio: number | null;
  pass: boolean | null;
}
const measured = ref<Measured[]>([]);
function measure() {
  measured.value = pairs.map((p) => {
    const fg = resolveColor(`var(${p.text})`);
    const bg = resolveColor(`var(${p.on})`);
    const ratio = fg && bg ? contrastRatio(fg, bg) : null;
    return { ...p, ratio, pass: ratio == null ? null : p.need === 0 ? null : ratio >= p.need };
  });
}
let frame = 0;
onMounted(() => (frame = requestAnimationFrame(measure)));
onBeforeUnmount(() => cancelAnimationFrame(frame));
// The scheme switch repaints the page; measure after it has.
watch(shown, () => requestAnimationFrame(() => requestAnimationFrame(measure)));
</script>

<template>
  <UiSection id="tokens" title="Spacing">
    <template #note>A 4 px grid. eos's own gaps are aliased onto it (theme.ts).</template>
    <div class="kit-spacing">
      <div v-for="s in spacing" :key="s" class="kit-spacing-row">
        <code class="mono">{{ s }}</code>
        <span class="kit-spacing-bar" :style="{ width: `var(${s})` }" />
      </div>
    </div>
  </UiSection>

  <UiSection title="Type">
    <template #note>Two families. Figures are mono, words are sans; nothing in between.</template>
    <div class="kit-type">
      <div v-for="[token, sample, spec] in type" :key="token" class="kit-type-row">
        <code class="mono kit-token">{{ token }}</code>
        <span :style="{ font: `var(${token})` }">{{ sample }}</span>
        <span class="muted kit-spec">{{ spec }}</span>
      </div>
    </div>
  </UiSection>

  <UiSection title="Colour roles">
    <template #note>Components pick a role, never a hue. Cognac is the one action and the selection.</template>
    <div class="kit-swatches">
      <div v-for="c in [...surfaces, ...inks, ...lines, ...roles, ...washes]" :key="c" class="kit-swatch">
        <span class="kit-chip" :style="{ background: `var(${c})` }" />
        <code class="mono">{{ c }}</code>
      </div>
    </div>
  </UiSection>

  <UiSection title="Contrast" :level="2">
    <template #note>Measured on this page in the scheme shown. Text needs 4.5, graphics 3.</template>
    <UiTable
      label="Contrast of text pairs"
      :columns="[
        { key: 'note', label: 'Pair' },
        { key: 'pair', label: 'Tokens', hide: 'sm' },
        { key: 'ratio', label: 'Ratio', fig: true },
        { key: 'need', label: 'Needs', fig: true, hide: 'sm' },
        { key: 'pass', label: '' },
      ]"
      :rows="measured.map((m) => ({ ...m, pair: `${m.text} on ${m.on}` }))"
      row-key="id"
    >
      <template #cell-ratio="{ row }">{{ row.ratio == null ? "–" : row.ratio.toFixed(2) }}</template>
      <template #cell-need="{ row }">{{ row.need || "–" }}</template>
      <template #cell-pass="{ row }">
        <span v-if="row.pass != null" :class="row.pass ? 'kit-pass' : 'kit-fail'" :data-pass="row.pass">{{
          row.pass ? "Pass" : "Fail"
        }}</span>
      </template>
    </UiTable>
  </UiSection>

  <UiSection title="Heights, radii, icons, layout">
    <div class="kit-grid">
      <div>
        <h3 class="kit-sub">Heights</h3>
        <div v-for="h in heights" :key="h" class="kit-spacing-row">
          <code class="mono">{{ h }}</code>
          <span class="kit-height" :style="{ height: `var(${h})` }" />
        </div>
      </div>
      <div>
        <h3 class="kit-sub">Radii</h3>
        <div class="kit-radii">
          <span class="kit-radius" style="border-radius: var(--r-0)"><code class="mono">--r-0</code></span>
          <span class="kit-radius" style="border-radius: var(--r-1)"><code class="mono">--r-1</code></span>
          <span class="kit-radius" style="border-radius: var(--r-pill)"><code class="mono">pill</code></span>
        </div>
        <h3 class="kit-sub">Icons</h3>
        <div class="kit-icons">
          <span><PhGear style="font-size: var(--i-sm)" /><code class="mono">14</code></span>
          <span><PhTray style="font-size: var(--i-md)" /><code class="mono">16</code></span>
          <span><PhGameController style="font-size: var(--i-lg)" /><code class="mono">20</code></span>
          <span><PhCheckCircle style="font-size: var(--i-xl)" /><code class="mono">28</code></span>
        </div>
        <h3 class="kit-sub">Layout</h3>
        <div v-for="w in widths" :key="w" class="kit-spacing-row">
          <code class="mono">{{ w }}</code>
          <span class="kit-width" :style="{ width: `min(var(${w}), 100%)` }" />
        </div>
      </div>
    </div>
  </UiSection>
</template>

<style scoped>
.kit-spacing,
.kit-type {
  display: grid;
  gap: var(--sp-2);
}

.kit-spacing-row {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
}

.kit-spacing-row code {
  flex: none;
  width: 8rem;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.kit-spacing-bar {
  height: var(--sp-3);
  background: var(--c-act);
}

.kit-height {
  display: block;
  width: 3rem;
  background: var(--c-sunk);
  outline: var(--e-stroke-hair) solid var(--c-line-strong);
}

.kit-width {
  display: block;
  height: var(--sp-3);
  background: var(--c-sunk);
  outline: var(--e-stroke-hair) solid var(--c-line-strong);
}

.kit-type-row {
  display: grid;
  grid-template-columns: 9rem minmax(0, 1fr) auto;
  align-items: baseline;
  gap: var(--sp-3);
}

.kit-token {
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.kit-spec {
  font: var(--t-meta);
}

.kit-swatches {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr));
  gap: var(--sp-2) var(--sp-3);
}

.kit-swatch {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  min-width: 0;
}

.kit-swatch code {
  min-width: 0;
  overflow: hidden;
  color: var(--c-ink-2);
  font: var(--t-meta);
  text-overflow: ellipsis;
}

.kit-chip {
  flex: none;
  width: 2rem;
  height: 1.5rem;
  border-radius: var(--r-1);
  outline: var(--e-stroke-hair) solid var(--c-line-strong);
}

.kit-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
  gap: var(--sp-5);
}

.kit-sub {
  margin: var(--sp-3) 0 var(--sp-2);
  font: var(--t-body-strong);
}

.kit-sub:first-child {
  margin-block-start: 0;
}

.kit-radii,
.kit-icons {
  display: flex;
  align-items: flex-end;
  gap: var(--sp-3);
}

.kit-radius {
  display: grid;
  place-items: center;
  width: 4rem;
  height: 3rem;
  background: var(--c-sunk);
  outline: var(--e-stroke-hair) solid var(--c-line-strong);
}

.kit-icons span {
  display: grid;
  justify-items: center;
  gap: var(--sp-1);
}

.kit-pass {
  color: var(--c-win);
  font-weight: 600;
}

.kit-fail {
  color: var(--c-loss);
  font-weight: 600;
}
</style>
