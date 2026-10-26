<script setup lang="ts">
import { computed } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ESpinner } from "@eos-ui/vue/components/ESpinner";
import { ETag } from "@eos-ui/vue/components/ETag";
import OpponentName from "../lichess/OpponentName.vue";
import type { PlayerStats } from "../../api/queries";
import { nameOf, sampleOf } from "./compare";

const props = defineProps<{
  player: PlayerStats;
  /** Rows this column leads, out of the rows that had a leader; 0 when nobody was compared. */
  leads: number;
  contested: number;
  importing: boolean;
  reviewing: boolean;
  locked: boolean;
  /** The newest games asked for. */
  limit: number;
}>();
defineEmits<{ import: []; review: [] }>();

const sample = computed(() => sampleOf(props.player));
const who = computed(() => (props.player.player === "me" ? "You" : props.player.player === "bot" ? "Your bot" : ""));
const progress = computed(() => `${props.player.reviewed} of ${props.player.games} reviewed`);
</script>

<template>
  <div class="head">
    <OpponentName :name="nameOf(player)" :title="player.title" :rating="player.rating" />
    <span class="line">
      <template v-if="who">{{ who }}<template v-if="contested"> · </template></template>
      <template v-if="contested">leads {{ leads }} of {{ contested }}</template>
    </span>

    <p v-if="importing" class="state"><ESpinner size="sm" silent /> Importing…</p>
    <EButton v-else-if="!player.games" variant="outline" size="sm" :disabled="locked" @click="$emit('import')">
      Import last {{ limit }} games
    </EButton>
    <template v-else>
      <p class="state num">
        {{ progress }}
        <ETag v-if="sample === 'few'" size="sm" tone="warning" label="Few games" />
      </p>
      <p v-if="player.pending_review && reviewing" class="state"><ESpinner size="sm" silent /> Reviewing…</p>
      <EButton
        v-else-if="player.pending_review"
        variant="outline"
        size="sm"
        :disabled="locked"
        @click="$emit('review')"
      >
        Review {{ player.pending_review }} {{ player.pending_review === 1 ? "game" : "games" }}
      </EButton>
    </template>
  </div>
</template>

<style scoped>
.head {
  display: grid;
  justify-items: start;
  gap: var(--e-gap-tight);
  font-weight: 400;
  text-align: start;
}

.line,
.state {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug);
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
