<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import type { LeaderboardEntry, Speed } from "../api/types";
import { formatElo } from "../format";
import { usePlayers } from "../stores/players";
import UiEmpty from "../ui/UiEmpty.vue";
import UiPlayer from "../ui/UiPlayer.vue";
import UiSeg from "../ui/UiSeg.vue";
import UiTable from "../ui/UiTable.vue";

/** Lichess's leaderboard, one tap to put someone in the comparison. */
const props = defineProps<{ taken: string[]; full: boolean }>();
const emit = defineEmits<{ add: [key: string] }>();

const players = usePlayers();
const speed = ref<Speed>("blitz");
const failed = ref(false);
const rows = computed(() => players.top[speed.value] ?? []);
const loading = computed(() => !players.top[speed.value] && !failed.value);

async function load(s: Speed) {
  failed.value = false;
  if (players.top[s]) return;
  try {
    await players.loadTop(s);
  } catch {
    failed.value = true;
  }
}
watch(speed, load, { immediate: true });

const key = (username: string) => `lichess:${username.toLowerCase()}`;
const has = (username: string) => props.taken.includes(key(username));
const ref_ = (p: LeaderboardEntry) => ({ name: p.username, key: key(p.username), title: p.title ?? null });

const columns = [
  { key: "rank", label: "#", fig: true, width: "2rem" },
  { key: "name", label: "Player" },
  { key: "rating", label: "Rating", fig: true },
  { key: "progress", label: "This week", fig: true, hide: "sm" as const },
  { key: "add", label: "Add", width: "4.5rem", align: "end" as const },
];
const table = computed(() => rows.value.map((p, i) => ({ ...p, rank: i + 1, id: p.key })));
</script>

<template>
  <div class="top">
    <UiSeg
      v-model="speed"
      label="Leaderboard"
      size="sm"
      :options="[
        { value: 'bullet', label: 'Bullet' },
        { value: 'blitz', label: 'Blitz' },
        { value: 'rapid', label: 'Rapid' },
      ]"
    />
    <UiEmpty v-if="failed" tone="error" dense title="Lichess did not answer.">
      <template #actions>
        <EButton variant="quiet" size="sm" @click="load(speed)">Try again</EButton>
      </template>
    </UiEmpty>
    <UiTable v-else label="Leaderboard" :columns="columns" :rows="table" row-key="id" :loading="loading">
      <template #cell-name="{ row }"><UiPlayer :seat="ref_(row)" link /></template>
      <template #cell-progress="{ row }">{{ formatElo(row.progress) }}</template>
      <template #cell-add="{ row }">
        <EButton
          variant="quiet"
          size="xs"
          :disabled="full || has(row.username)"
          @click="emit('add', key(row.username))"
        >
          {{ has(row.username) ? "Added" : "Add" }}
        </EButton>
      </template>
    </UiTable>
  </div>
</template>

<style scoped>
.top {
  display: grid;
  gap: var(--sp-2);
  min-width: 0;
}
</style>
