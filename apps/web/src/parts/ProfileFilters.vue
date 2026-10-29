<script setup lang="ts">
import { computed } from "vue";
import { useMedia } from "../media";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import type { ProfileFilter, Speed } from "../api/types";
import { coverage, speedName, WINDOWS, windowName } from "../format";

/**
 * Which games a profile adds up: the newest 100, 200, 500 or all of them, one speed or all, rated or casual or both.
 * With `games` (how many the profile counted) it says in a line what the figures cover.
 */
const props = defineProps<{ games?: number; whose?: string }>();
const filter = defineModel<ProfileFilter>({ required: true });

const speed = computed({
  get: () => filter.value.speeds?.[0] ?? "all",
  set: (v: string | undefined) => (filter.value = { ...filter.value, speeds: !v || v === "all" ? [] : [v as Speed] }),
});
const rated = computed({
  get: () => (filter.value.rated == null ? "both" : filter.value.rated ? "rated" : "casual"),
  set: (v: string | undefined) =>
    (filter.value = { ...filter.value, rated: v === "rated" ? true : v === "casual" ? false : null }),
});
const span = computed({
  get: () => String(filter.value.limit ?? 200),
  set: (v: string | undefined) => v && (filter.value = { ...filter.value, limit: Number(v) }),
});
/** On a phone each switch takes the sheet's width, its options sharing it, instead of two ragged rows. */
const phone = useMedia("(width < 36rem)");
const speeds = [
  { value: "all", label: "All speeds" },
  ...(["bullet", "blitz", "rapid", "classical"] as const).map((s) => ({ value: s, label: speedName[s] })),
];
const windows = WINDOWS.map((n) => ({ value: String(n), label: windowName(n) }));
const covers = computed(() =>
  props.games ? `Figures are from ${coverage(filter.value.limit ?? 200, props.games, props.whose)}.` : "",
);
</script>

<template>
  <div class="filters">
    <ESelect v-model="span" label="Games" size="sm" :options="windows" class="seg" />
    <ESegmented v-model="speed" label="Speed" hide-label size="sm" :stretch="phone" class="seg" :options="speeds" />
    <ESegmented
      v-model="rated"
      label="Rated"
      hide-label
      size="sm"
      :stretch="phone"
      class="seg"
      :options="[
        { value: 'both', label: 'Rated and casual' },
        { value: 'rated', label: 'Rated' },
        { value: 'casual', label: 'Casual' },
      ]"
    />
    <p v-if="covers" class="sec-note covers">{{ covers }}</p>
  </div>
</template>

<style scoped>
.filters {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-snug);
}

.covers {
  flex-basis: 100%;
  margin: 0;
}

@media (width < 36rem) {
  .seg {
    flex: 1 1 100%;
  }
}
</style>
