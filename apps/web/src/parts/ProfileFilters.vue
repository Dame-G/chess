<script setup lang="ts">
import { computed } from "vue";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import type { ProfileFilter, Speed } from "../api/types";
import { speedName } from "../format";

/** Which games a profile adds up: one speed or all, rated or casual or both. */
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
const speeds = [
  { value: "all", label: "All speeds" },
  ...(["bullet", "blitz", "rapid", "classical"] as const).map((s) => ({ value: s, label: speedName[s] })),
];
</script>

<template>
  <div class="filters">
    <ESegmented v-model="speed" label="Speed" hide-label size="sm" :options="speeds" />
    <ESegmented
      v-model="rated"
      label="Rated"
      hide-label
      size="sm"
      :options="[
        { value: 'both', label: 'Rated and casual' },
        { value: 'rated', label: 'Rated' },
        { value: 'casual', label: 'Casual' },
      ]"
    />
  </div>
</template>

<style scoped>
.filters {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-snug);
}

/* On a phone each switch takes the sheet's width, its options sharing it, instead of two ragged rows. */
@media (width < 36rem) {
  .filters > :deep(*) {
    flex: 1 1 100%;
  }

  .filters :deep(.e-seg) {
    display: flex;
    inline-size: 100%;
  }

  .filters :deep(.e-seg-opt) {
    display: flex;
    flex: 1 1 auto;
  }

  .filters :deep(.e-seg-text) {
    flex: 1;
    justify-content: center;
  }
}
</style>
