<script setup lang="ts">
import { computed } from "vue";
import { PhX } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import OpponentName from "../lichess/OpponentName.vue";
import { useLichessPlayer, type PlayerStats } from "../../api/queries";
import { speedName } from "../../lichess";
import { nameOf } from "./compare";

const props = defineProps<{
  /** `me`, `bot` or a Lichess username. */
  id: string;
  /** Its column of the comparison, once there is one. */
  stats?: PlayerStats;
}>();
defineEmits<{ remove: [] }>();

const named = computed(() => props.id !== "me" && props.id !== "bot");
const { state: profile } = useLichessPlayer(() => (named.value ? props.id : ""));

/** The rating of the speed they play most. */
const rating = computed(() => {
  if (!named.value) return props.stats?.rating != null ? { value: props.stats.rating, speed: "" } : undefined;
  const ratings = profile.value.data?.ratings;
  if (!ratings) return undefined;
  let best: { value: number; speed: string; games: number } | undefined;
  for (const [speed, r] of Object.entries(ratings))
    if (r && (!best || r.games > best.games)) best = { value: r.rating, speed: speedName(speed), games: r.games };
  return best;
});
const title = computed(() => props.stats?.title ?? profile.value.data?.title);
const name = computed(
  () =>
    profile.value.data?.username ??
    (props.stats ? nameOf(props.stats) : props.id === "me" ? "You" : props.id === "bot" ? "Your bot" : props.id),
);
const line = computed(() => {
  const s = props.stats;
  if (!s) return profile.value.status === "error" ? "Not found on Lichess" : "";
  if (!s.games) return "No games here yet";
  return `${s.games} ${s.games === 1 ? "game" : "games"}, ${s.reviewed} reviewed`;
});
</script>

<template>
  <li class="entry">
    <div class="who">
      <OpponentName :name="name" :title="title" :rating="rating?.value" />
      <span class="line num">
        <template v-if="stats && id === 'me'">You · </template>
        <template v-else-if="stats && id === 'bot'">Your bot · </template>
        <template v-else-if="rating?.speed">{{ rating.speed }} · </template>{{ line }}
      </span>
    </div>
    <EButton
      v-if="id !== 'me'"
      variant="quiet"
      size="sm"
      :icon="PhX"
      :aria-label="`Remove ${name} from the comparison`"
      @click="$emit('remove')"
    />
  </li>
</template>

<style scoped>
.entry {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-snug);
  min-block-size: 2.75rem;
  padding: var(--e-gap-snug) var(--e-gap-snug) var(--e-gap-snug) var(--ws-pad);
}

.who {
  display: grid;
  min-inline-size: 0;
}

.line {
  overflow: hidden;
  color: var(--e-muted);
  font: var(--e-type-small);
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
