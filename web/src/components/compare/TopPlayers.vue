<script setup lang="ts">
import { PhCheck, PhPlus } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import OpponentName from "../lichess/OpponentName.vue";
import { useTopPlayers } from "../../api/queries";
import { speedName } from "../../lichess";
import { stored } from "../../stored";

const props = defineProps<{ chosen: readonly string[] }>();
defineEmits<{ add: [username: string] }>();

const perf = stored<string>("compare.top", "blitz");
const perfs = ["bullet", "blitz", "rapid"].map((value) => ({ value, label: speedName(value) }));
const { state } = useTopPlayers(perf);
const taken = (name: string) => props.chosen.some((c) => c.toLowerCase() === name.toLowerCase());
</script>

<template>
  <section class="top" aria-labelledby="top-title">
    <div class="head">
      <h3 id="top-title" class="part-title">Top {{ perf }} players now</h3>
      <ESegmented v-model="perf" label="Speed" hide-label size="sm" :options="perfs" />
    </div>
    <ESkeleton v-if="state.status === 'pending'" :lines="5" label="Loading the leaderboard" />
    <ENotice v-else-if="state.status === 'error'" tone="warning" title="No leaderboard" :role="false">
      {{ state.error?.message }}
    </ENotice>
    <ol v-else class="list">
      <li v-for="(p, i) in state.data" :key="p.id" class="row">
        <span class="rank num">{{ i + 1 }}</span>
        <OpponentName :name="p.username" :title="p.title" :rating="p.rating" />
        <span :class="['delta num', p.progress > 0 && 'up', p.progress < 0 && 'down']">
          {{ p.progress > 0 ? "+" : "" }}{{ p.progress }}
        </span>
        <EButton
          v-if="taken(p.username)"
          variant="quiet"
          size="sm"
          :icon="PhCheck"
          :aria-label="`${p.username} is in the comparison`"
          disabled
        />
        <EButton
          v-else
          variant="quiet"
          size="sm"
          :icon="PhPlus"
          :aria-label="`Compare with ${p.username}`"
          @click="$emit('add', p.username)"
        />
      </li>
    </ol>
  </section>
</template>

<style scoped>
.top {
  display: grid;
  gap: var(--e-gap-snug);
}

.head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-snug);
}

.list {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: grid;
  grid-template-columns: 1.25rem minmax(0, 1fr) auto auto;
  align-items: center;
  gap: var(--e-gap-snug);
  min-block-size: 2rem;
}

.rank,
.delta {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.delta.up {
  color: var(--e-success);
}

.delta.down {
  color: var(--e-danger);
}
</style>
