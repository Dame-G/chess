<script setup lang="ts">
import { computed, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { plural, reviewCost } from "../format";
import { usePlayers } from "../stores/players";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import { useWork } from "../stores/work";

/**
 * What a profile can do next with its games, as one filled button: import the player's games when none are here,
 * else review the ones not reviewed yet. Both are analysis, so they are off (with one calm line why) while you play on
 * Lichess. `mine` is You, whose games are already here.
 */
const props = defineProps<{
  playerKey: string;
  name: string;
  games: number;
  pending: number;
  mine?: boolean;
  /** Another action on the screen is the filled one. */
  quiet?: boolean;
}>();

const players = usePlayers();
const gamesStore = useGames();
const policy = usePolicy();
const work = useWork();
const toast = useToast();

const IMPORT = 50;
const REVIEW_AT_A_TIME = 20;
const busy = ref(false);

/** Work of this kind for this player that is queued or running, so the button does not ask twice. */
const underway = computed(() =>
  [...work.running, ...work.queued].find(
    (w) =>
      (w.kind === "import" || w.kind === "review") && w.subject.type === "player" && w.subject.key === props.playerKey,
  ),
);
const next = computed<"import" | "review" | null>(() =>
  props.games === 0 && !props.mine ? "import" : props.pending > 0 ? "review" : null,
);
const toReview = computed(() => Math.min(props.pending, REVIEW_AT_A_TIME));

async function go() {
  busy.value = true;
  try {
    if (next.value === "import") await players.importHistory(props.playerKey, { max: IMPORT });
    else await gamesStore.reviewPlayer(props.playerKey, toReview.value);
  } catch (e) {
    toast.show({ title: "That did not start", message: e instanceof Error ? e.message : String(e), tone: "warning" });
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div v-if="next" class="bar">
    <p class="line">
      <template v-if="underway">{{ underway.title }}. It will show here when it is done.</template>
      <template v-else-if="next === 'import'">
        None of {{ name }}'s games are here yet. Reading the last {{ IMPORT }} from Lichess takes a moment.
      </template>
      <template v-else>
        {{ plural(pending, "game") }} not reviewed yet
        <span class="cost"
          >— {{ reviewCost(toReview) }} for {{ pending > toReview ? `the newest ${toReview}` : "all of them" }}</span
        >
      </template>
    </p>
    <EButton
      v-if="!underway"
      :variant="quiet ? 'outline' : 'primary'"
      :loading="busy"
      :disabled="!policy.allowed"
      @click="go"
    >
      {{
        next === "import"
          ? `Import their last ${IMPORT} games`
          : pending > toReview
            ? `Review ${toReview} games`
            : "Review them"
      }}
    </EButton>
    <p v-if="!policy.allowed && !underway" class="sec-note why" role="status">
      {{ policy.reason }}. This works again when your game ends.
    </p>
  </div>
</template>

<style scoped>
.bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug) var(--e-gap-base);
  padding: var(--e-gap-snug);
  border: var(--line);
  border-radius: var(--e-radius-sm);
  background: var(--e-surface-tint, transparent);
}

.line {
  flex: 1 1 14rem;
  margin: 0;
  min-width: 0;
}

.cost {
  color: var(--e-muted);
}

.why {
  flex-basis: 100%;
}
</style>
