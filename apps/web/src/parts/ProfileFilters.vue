<script setup lang="ts">
import { computed } from "vue";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import type { ProfileFilter, Speed } from "../api/types";
import { coverage, speedName, WINDOWS, windowName } from "../format";
import UiSeg from "../ui/UiSeg.vue";
import UiToolbar from "../ui/UiToolbar.vue";

/**
 * Which games a profile adds up: the newest 100, 200, 500 or all of them, one speed or all, rated or casual or both.
 * It sticks to the top of the sheet's tab, under the tab row (`tabs`), so the figures below never scroll away from what
 * they are filtered by. With `games` (how many the profile counted) it says in a line what the figures cover.
 */
const props = defineProps<{ games?: number; whose?: string; tabs?: boolean }>();
const filter = defineModel<ProfileFilter>({ required: true });

const speed = computed({
  get: () => filter.value.speeds?.[0] ?? "all",
  set: (v: string) => (filter.value = { ...filter.value, speeds: !v || v === "all" ? [] : [v as Speed] }),
});
const rated = computed({
  get: () => (filter.value.rated == null ? "both" : filter.value.rated ? "rated" : "casual"),
  set: (v: string) => (filter.value = { ...filter.value, rated: v === "rated" ? true : v === "casual" ? false : null }),
});
const span = computed({
  get: () => String(filter.value.limit ?? 200),
  set: (v: string | undefined) => v && (filter.value = { ...filter.value, limit: Number(v) }),
});
const speeds = [
  { value: "all", label: "All speeds" },
  ...(["bullet", "blitz", "rapid", "classical"] as const).map((s) => ({ value: s, label: speedName[s] })),
];
const kinds = [
  { value: "both", label: "All" },
  { value: "rated", label: "Rated" },
  { value: "casual", label: "Casual" },
];
const windows = WINDOWS.map((n) => ({ value: String(n), label: windowName(n) }));
const covers = computed(() =>
  props.games ? `Figures are from ${coverage(filter.value.limit ?? 200, props.games, props.whose)}.` : "",
);
</script>

<template>
  <div class="filters" :class="{ 'filters--tabs': tabs }">
    <UiToolbar label="Which games to add up" class="bar">
      <UiSeg v-model="speed" label="Speed" size="sm" :options="speeds" />
      <UiSeg v-model="rated" label="Rated or casual" size="sm" :options="kinds" />
      <ESelect v-model="span" label="Games" hide-label size="sm" :options="windows" class="span" />
    </UiToolbar>
    <p v-if="covers" class="covers">{{ covers }}</p>
  </div>
</template>

<style scoped>
/* A sheet's padding is the toolbar's to bleed through, so the bar spans the pane and its hairline reaches both edges. */
.filters {
  position: sticky;
  inset-block-start: 0;
  z-index: var(--e-z-sticky);
  display: grid;
  gap: var(--sp-1);
  margin-inline: calc(-1 * var(--sp-3));
  padding: var(--sp-2) var(--sp-3);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
  background: var(--c-pane);
}

.filters--tabs {
  inset-block-start: var(--h-tab);
}

@media (width >= 56rem) {
  .filters {
    margin-inline: calc(-1 * var(--sp-4));
    padding-inline: var(--sp-4);
  }
}

.covers {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.span {
  min-width: 10rem;
}
</style>
