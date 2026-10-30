<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterLink } from "vue-router";
import { PhCaretRight } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import type { Profile } from "../api/types";
import { THEMES, trainable } from "../board/themes";
import { plural } from "../format";
import { useStartDrills } from "../shell/drillFlow";
import { boardTo, spotKey, usePlayOut } from "../sheets/playOut";
import { countedText, eachText, lost, namedBy, oftenText, perGameText, shareText, totals } from "../sheets/themes";
import { useDrills } from "../stores/drills";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSection from "../ui/UiSection.vue";
import UiTable from "../ui/UiTable.vue";

/**
 * Where your games went wrong: every mistake and blunder in your reviewed games sorted by what it was about (hanging
 * pieces, missed tactics, time trouble, and so on), the costliest first, with how often it happens and what it costs per
 * game. A row opens to what the rule saw, the causes named most often, the costliest examples (each opens its game, or
 * is played out against an engine) and a way to drill that theme. A mistake that no rule explains is "Other".
 */
const props = defineProps<{ profile: Profile }>();

const drills = useDrills();
const startDrills = useStartDrills();
const { starting, playOut } = usePlayOut();

const rows = computed(() => props.profile.themes.map((t) => ({ ...t, name: THEMES[t.kind].name })));
const sum = computed(() => totals(props.profile.themes));
const columns = [
  { key: "name", label: "Theme" },
  { key: "count", label: "Mistakes", fig: true },
  { key: "per_game", label: "A game", fig: true, hide: "sm" as const },
  { key: "each", label: "Costs each", fig: true },
  { key: "share", label: "Share", fig: true, hide: "sm" as const },
];

/** The costliest theme is open to begin with; what you open stays open. */
const expanded = ref<string[]>([]);
watch(
  rows,
  (now, before) => {
    if (!before?.length && now[0]) expanded.value = [now[0].kind];
  },
  { immediate: true },
);
function toggle(row: { kind: string }) {
  expanded.value = expanded.value.includes(row.kind)
    ? expanded.value.filter((k) => k !== row.kind)
    : [...expanded.value, row.kind];
}

const moveNo = (ply: number, san: string) => `${Math.ceil(ply / 2)}${ply % 2 ? "." : "…"} ${san}`;
</script>

<template>
  <UiSection id="y-themes" title="Where you go wrong">
    <template #note>
      Your mistakes and blunders in {{ plural(profile.reviewed, "reviewed game") }}, by what they were about. Costs each
      is the win percentage one of them gives away; share is the part of everything lost.
    </template>
    <UiEmpty
      v-if="profile.reviewed === 0"
      dense
      title="Review some of your games and your mistakes are sorted by theme here."
    />
    <UiEmpty v-else-if="!rows.length" dense title="No mistakes or blunders in your reviewed games." />
    <UiTable
      v-else
      label="Your mistakes by theme"
      :columns="columns"
      :rows="rows"
      row-key="kind"
      :expanded="expanded"
      @select="toggle"
    >
      <template #cell-name="{ row }">
        <span class="name">
          <PhCaretRight class="chev" :class="{ 'is-open': expanded.includes(row.kind) }" aria-hidden="true" />
          {{ row.name }}
        </span>
      </template>
      <template #cell-per_game="{ row }">{{ perGameText(row.count, profile.reviewed) }}</template>
      <template #cell-each="{ row }">{{ eachText(row) }}</template>
      <template #cell-share="{ row }">
        <span class="share">
          <span class="bar" aria-hidden="true"
            ><span class="fill" :style="{ inlineSize: `${row.share * 100}%` }"
          /></span>
          {{ shareText(row.share) }}
        </span>
      </template>
      <template #row-aside="{ row }">
        <div class="detail">
          <p class="meta">{{ oftenText(row) }}. {{ THEMES[row.kind].evidence }}</p>
          <p v-if="namedBy(row)" class="meta">
            {{ namedBy(row)?.label }}: {{ countedText(namedBy(row)?.items ?? []) }}
          </p>
          <ul class="examples" :aria-label="`Examples of ${THEMES[row.kind].name.toLowerCase()}`">
            <li v-for="s in row.examples" :key="spotKey(s)" class="example">
              <RouterLink class="example-link" :to="boardTo(s)">
                <span class="mono played">{{ moveNo(s.ply, s.played) }}</span>
                <span class="better">
                  better <span class="mono">{{ s.best }}</span>
                </span>
                <span class="swing num">{{ lost(s.swing) }}</span>
                <span class="meta where">
                  <template v-if="s.detail">{{ s.detail }} · </template>{{ s.opening ?? "Unnamed opening" }},
                  {{ s.phase }}
                </span>
              </RouterLink>
              <EButton variant="outline" size="sm" :loading="starting === spotKey(s)" @click="playOut(s)">
                Play this out
              </EButton>
            </li>
          </ul>
          <div v-if="trainable(row.kind) && drills.available" class="actions">
            <EButton variant="outline" size="sm" @click="startDrills({ type: 'theme', theme: row.kind })">
              Drill {{ THEMES[row.kind].name.toLowerCase() }}
            </EButton>
          </div>
        </div>
      </template>
      <template #foot>
        <span class="foot">
          All themes: {{ plural(sum.count, "mistake") }}, {{ perGameText(sum.count, profile.reviewed) }} a game, costing
          {{ eachText({ count: sum.count, swing: sum.swing }, false) }} each
        </span>
      </template>
    </UiTable>
  </UiSection>
</template>

<style scoped>
.name {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
}

.chev {
  flex: none;
  color: var(--c-ink-3);
  font-size: var(--i-sm);
  transition: transform var(--m-move) var(--m-ease);
}

.chev.is-open {
  transform: rotate(90deg);
}

@media (prefers-reduced-motion: reduce) {
  .chev {
    transition: none;
  }
}

.share {
  display: inline-flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--sp-2);
}

.bar {
  inline-size: calc(var(--sp-6) * 2);
  block-size: var(--sp-1);
  background: var(--c-sunk);
}

.fill {
  display: block;
  block-size: 100%;
  background: var(--c-ink-2);
}

.detail {
  display: grid;
  gap: var(--sp-2);
  padding: var(--sp-2) var(--sp-3) var(--sp-3) var(--sp-4);
}

.meta {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.examples {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.example {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-3);
  padding-block: var(--sp-1);
}

.example + .example {
  border-block-start: var(--e-stroke-hair) solid var(--c-line);
}

.example-link {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0 var(--sp-3);
  min-width: 0;
  color: inherit;
  text-decoration: none;
}

.example-link:hover .played {
  text-decoration: underline;
}

.played {
  font-weight: 600;
}

.better {
  color: var(--c-ink-2);
}

.swing {
  color: var(--rv-blunder);
  font: var(--t-fig);
}

.where {
  flex-basis: 100%;
}

.actions {
  display: flex;
  gap: var(--sp-2);
}

.foot {
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
