<script setup lang="ts">
import type { Profile } from "../api/types";
import { formatPercent } from "../format";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSection from "../ui/UiSection.vue";
import UiTable from "../ui/UiTable.vue";
import WdlBar from "./WdlBar.vue";

/** The openings a player has met, how often, and how they scored in each. */
defineProps<{ profile: Profile }>();

const columns = [
  { key: "name", label: "Opening" },
  { key: "games", label: "Games", fig: true },
  { key: "record", label: "Record", width: "30%", hide: "sm" as const },
  { key: "score", label: "Score", fig: true },
];
</script>

<template>
  <UiSection title="Openings played">
    <UiTable
      v-if="profile.openings.length"
      label="Openings played"
      :columns="columns"
      :rows="profile.openings"
      row-key="name"
    >
      <template #cell-games="{ row }">{{ row.results.games }}</template>
      <template #cell-record="{ row }"><WdlBar :wdl="row.results" :label="row.name" /></template>
      <template #cell-score="{ row }">{{ formatPercent(row.results.score, 0) }}</template>
    </UiTable>
    <UiEmpty v-else dense title="No openings yet. They show once there are games to read them from." />
  </UiSection>
</template>
