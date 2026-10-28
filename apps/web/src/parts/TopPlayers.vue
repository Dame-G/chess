<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterLink } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EList } from "@eos-ui/vue/components/EList";
import { EListRow } from "@eos-ui/vue/components/EListRow";
import { ENotice } from "@eos-ui/vue/components/ENotice";
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
</script>

<template>
  <div class="top">
    <ESegmented
      v-model="speed"
      label="Leaderboard"
      hide-label
      size="sm"
      :options="[
        { value: 'bullet', label: 'Bullet' },
        { value: 'blitz', label: 'Blitz' },
        { value: 'rapid', label: 'Rapid' },
      ]"
    />
    <ENotice v-if="failed" tone="warning" title="Lichess did not answer" retry @retry="load(speed)" />
    <ESkeleton v-else-if="!rows" :lines="5" label="Loading the leaderboard" />
    <EList v-else :model-value="null" label="Leaderboard">
      <EListRow
        v-for="(p, i) in rows"
        :key="p.key"
        :value="p.key"
        :link-component="RouterLink"
        :to="`/players/${key(p.username)}`"
      >
        <template #leading>
          <span class="rank mono">{{ i + 1 }}</span>
        </template>
        <template #title>
          <span v-if="p.title" class="title">{{ p.title }}</span>
          {{ p.username }}
        </template>
        <template #trailing>
          <span class="rating mono">{{ p.rating }}</span>
          <span class="progress mono" :aria-label="`${p.progress ?? 0} this week`">{{
            signed(p.progress ?? 0, 0)
          }}</span>
        </template>
        <template #actions>
          <EButton variant="quiet" size="xs" :disabled="full || has(p.username)" @click="emit('add', key(p.username))">
            {{ has(p.username) ? "Added" : "Add" }}
          </EButton>
        </template>
      </EListRow>
    </EList>
  </div>
</template>

<style scoped>
.top {
  display: grid;
  gap: var(--e-gap-snug);
}

.rank {
  inline-size: 1.5rem;
  color: var(--e-muted);
  font-size: 0.8125rem;
}

/* The rating and the week's change sit side by side, each in its own column. */
.rating,
.progress {
  inline-size: 3rem;
  text-align: end;
}

.progress {
  font-size: 0.8125rem;
}

.title {
  margin-inline-end: 0.25rem;
  color: var(--e-primary);
  font: var(--e-type-small);
  font-weight: 700;
}
</style>
