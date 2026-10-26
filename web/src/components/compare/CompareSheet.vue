<script setup lang="ts">
import { computed } from "vue";
import { EVisuallyHidden } from "@eos-ui/vue/components/EVisuallyHidden";
import Term from "../Term.vue";
import ColumnHead from "./ColumnHead.vue";
import { buildSheet, classes, glyphOf, nameOf, sampleOf, type Cell } from "./compare";
import type { PlayerStats } from "../../api/queries";

const props = defineProps<{
  players: readonly PlayerStats[];
  importing: ReadonlySet<string>;
  reviewing: boolean;
  locked: boolean;
  limit: number;
}>();
defineEmits<{ import: [player: string]; review: [player: string] }>();

const sheet = computed(() => buildSheet(props.players));
const thin = (i: number) => sampleOf(props.players[i]!) === "few";
const label = (c: Cell) =>
  c.mix
    ? classes
        .map((k) => `${k.label} ${Math.round((c.mix!.find((m) => m.key === k.key)?.share ?? 0) * 100)}%`)
        .join(", ")
    : "";
</script>

<template>
  <div class="scroll" tabindex="0" role="region" aria-label="Comparison of the players">
    <table class="sheet">
      <caption class="sr">
        Statistics of
        {{
          players.map(nameOf).join(", ")
        }}
      </caption>
      <thead>
        <tr>
          <td class="corner"></td>
          <th v-for="(p, i) in players" :key="p.player" scope="col">
            <ColumnHead
              :player="p"
              :leads="sheet.leads[i]!"
              :contested="players.length > 1 ? sheet.contested : 0"
              :importing="importing.has(p.player.toLowerCase())"
              :reviewing="reviewing"
              :locked="locked"
              :limit="limit"
              @import="$emit('import', p.player)"
              @review="$emit('review', p.player)"
            />
          </th>
        </tr>
      </thead>
      <tbody v-for="g in sheet.groups" :key="g.id">
        <tr class="group">
          <th scope="colgroup" :colspan="players.length + 1">
            <span class="part-title">{{ g.title }}</span>
            <span v-if="g.hint" class="hint">{{ g.hint }}</span>
          </th>
        </tr>
        <tr v-for="r in g.rows" :key="r.id" :class="{ nested: r.nested }">
          <th scope="row" class="label">
            <span v-if="r.glyph" class="glyph" :style="{ color: `var(--q-${r.glyph})` }" aria-hidden="true">
              {{ glyphOf(r.glyph) }}
            </span>
            <Term v-if="r.term" :of="r.term" inline>{{ r.label }}</Term>
            <template v-else>{{ r.label }}</template>
          </th>
          <td v-for="(c, i) in r.cells" :key="i" :class="{ lead: c.lead, thin: thin(i) && g.id !== 'results' }">
            <span v-if="c.mix" class="mix" role="img" :aria-label="label(c)">
              <i v-for="m in c.mix" :key="m.key" :style="{ flexGrow: m.share, background: `var(--q-${m.key})` }"></i>
            </span>
            <template v-else>
              <span class="v num">{{ c.text }}</span>
              <span v-if="c.sub" class="sub num">{{ c.sub }}</span>
              <EVisuallyHidden v-if="c.lead"> (best in this row)</EVisuallyHidden>
              <span v-if="c.bar != null" class="bar" aria-hidden="true"
                ><i :style="{ inlineSize: `${c.bar * 100}%` }"></i
              ></span>
            </template>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.scroll {
  overflow-x: auto;
  overscroll-behavior-x: contain;
}

.sheet {
  --label: 9.5rem;
  --col: 9.5rem;

  inline-size: 100%;
  min-inline-size: calc(var(--label) + var(--col) * v-bind("players.length"));
  border-collapse: separate;
  border-spacing: 0;
  table-layout: fixed;
}

.sr {
  position: absolute;
  inline-size: 1px;
  block-size: 1px;
  overflow: hidden;
  clip-path: inset(50%);
}

th,
td {
  padding: var(--e-gap-snug) var(--e-gap-base);
  vertical-align: top;
  text-align: start;
}

thead th {
  padding-block: var(--e-gap-base);
  border-block-end: var(--ws-line);
}

/* The label column stays while the rest scrolls. */
.corner,
.label {
  inline-size: var(--label);
  position: sticky;
  inset-inline-start: 0;
  z-index: 1;
  background: var(--e-background);
}

.group th {
  padding-block: var(--e-gap-base) var(--e-gap-tight);
  border-block-end: var(--ws-line);
  background: var(--e-background);
}

.group .hint {
  margin-inline-start: var(--e-gap-snug);
  color: var(--e-muted);
  font: var(--e-type-small);
  text-transform: none;
}

.label {
  font: var(--e-type-small);
  font-weight: 500;
}

/* On a phone the labels wrap and the columns are as narrow as their numbers allow; the table scrolls sideways. */
@media (width < 40rem) {
  .sheet {
    --label: 6.5rem;
    --col: 8.5rem;
  }

  th,
  td {
    padding-inline: var(--e-gap-snug);
  }

  .nested .label {
    padding-inline-start: var(--e-gap-base);
  }
}

/* A defined term reads as a label, with a dotted line to say it explains itself. */
.label :deep(.e-toggletip-term) {
  color: inherit;
  font: inherit;
  text-decoration: underline dotted;
  text-decoration-color: color-mix(in oklab, var(--e-muted) 60%, transparent);
  text-underline-offset: 0.2em;
}

.nested .label {
  padding-inline-start: var(--e-gap-loose);
  color: var(--e-muted);
  font-weight: 400;
}

tbody tr:not(.group) > * {
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 28%, transparent);
}

td {
  position: relative;
}

.v {
  font-family: var(--e-font-mono);
  font-size: var(--e-text-sm);
}

.sub {
  margin-inline-start: var(--e-gap-snug);
  color: var(--e-muted);
  font: var(--e-type-small);
}

/* A leader: a tint and weight, and the words for those who do not see colour. */
td.lead {
  background: color-mix(in oklab, var(--e-primary) 9%, transparent);
}

td.lead .v {
  color: var(--e-primary);
  font-weight: 700;
}

/* A column that rests on a few games is quieter. */
td.thin .v {
  color: var(--e-muted);
}

.bar {
  display: block;
  inline-size: 100%;
  block-size: 0.1875rem;
  margin-block-start: 0.25rem;
  border-radius: 1px;
  background: color-mix(in oklab, var(--e-border) 40%, transparent);
}

.bar i {
  display: block;
  block-size: 100%;
  border-radius: inherit;
  background: color-mix(in oklab, var(--e-ink) 45%, transparent);
}

td.lead .bar i {
  background: var(--e-primary);
}

.mix {
  display: flex;
  inline-size: 100%;
  block-size: 0.625rem;
  overflow: hidden;
  border-radius: 2px;
  gap: 1px;
}

.mix i {
  display: block;
  flex-basis: 0;
  min-inline-size: 1px;
}

.glyph {
  display: inline-block;
  min-inline-size: 1.25rem;
  font-family: var(--e-font-mono);
  font-weight: 700;
}
</style>
