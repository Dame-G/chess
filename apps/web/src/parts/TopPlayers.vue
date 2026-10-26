<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import type { Speed } from "../api/types";
import { signed } from "../format";
import { usePlayers } from "../stores/players";

/** Lichess's leaderboard, one tap to put someone in the comparison. */
const props = defineProps<{ taken: string[]; full: boolean }>();
const emit = defineEmits<{ add: [key: string] }>();

const players = usePlayers();
const speed = ref<Speed>("blitz");
const failed = ref(false);
const rows = computed(() => players.top[speed.value]);

watch(
  speed,
  async (s) => {
    failed.value = false;
    if (players.top[s]) return;
    try {
      await players.loadTop(s);
    } catch {
      failed.value = true;
    }
  },
  { immediate: true },
);

const key = (username: string) => `lichess:${username.toLowerCase()}`;
const has = (username: string) => props.taken.includes(key(username));
</script>

<template>
  <div class="top">
    <ESegmented
      v-model="speed"
      label="Leaderboard"
      size="sm"
      :options="[
        { value: 'bullet', label: 'Bullet' },
        { value: 'blitz', label: 'Blitz' },
        { value: 'rapid', label: 'Rapid' },
      ]"
    />
    <p v-if="failed" class="sec-note">Lichess did not answer. Try again in a moment.</p>
    <ESkeleton v-else-if="!rows" :lines="5" label="Loading the leaderboard" />
    <ol v-else class="plain list">
      <li v-for="(p, i) in rows" :key="p.id" class="item">
        <span class="rank mono">{{ i + 1 }}</span>
        <span class="name">
          <span v-if="p.title" class="title">{{ p.title }}</span>
          {{ p.username }}
        </span>
        <span class="rating mono">{{ p.rating }}</span>
        <span class="progress mono" :aria-label="`${p.progress} this week`">{{ signed(p.progress, 0) }}</span>
        <EButton variant="quiet" size="sm" :disabled="full || has(p.username)" @click="emit('add', key(p.username))">
          {{ has(p.username) ? "Added" : "Add" }}
        </EButton>
      </li>
    </ol>
  </div>
</template>

<style scoped>
.top {
  display: grid;
  gap: var(--e-gap-snug);
}

.list {
  gap: 0;
}

.item {
  display: grid;
  grid-template-columns: 1.5rem minmax(0, 1fr) 3.25rem 2.5rem auto;
  align-items: center;
  gap: var(--e-gap-snug);
  padding-block: 0.125rem;
  border-block-end: var(--line);
}

.rank,
.progress {
  color: var(--e-muted);
  font-size: 0.8125rem;
}

.progress {
  text-align: end;
}

.title {
  margin-inline-end: 0.25rem;
  color: var(--e-primary);
  font: var(--e-type-small);
  font-weight: 700;
}
</style>
