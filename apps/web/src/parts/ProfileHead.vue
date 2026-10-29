<script setup lang="ts">
import { computed } from "vue";
import type { Player } from "../api/types";
import { formatNumber, speedName } from "../format";
import UiEmpty from "../ui/UiEmpty.vue";
import UiStat from "../ui/UiStat.vue";
import UiStats from "../ui/UiStats.vue";

/**
 * The hero strip of a profile: the Lichess rating at the speed they play most, large, and the other speeds beside it.
 * A rating that is still provisional is written `~1,345` and says so on hover, not as a trailing question mark.
 */
const props = defineProps<{ player: Player }>();

const ratings = computed(() =>
  (["bullet", "blitz", "rapid", "classical"] as const).flatMap((s) => {
    const r = props.player.ratings[s];
    return r
      ? [
          {
            speed: s,
            label: speedName[s],
            value: `${r.provisional ? "~" : ""}${formatNumber(r.rating)}`,
            hint: r.provisional ? "Provisional: too few games for a settled rating" : undefined,
            games: r.games,
          },
        ]
      : [];
  }),
);
/** The speed they play most, else the first they have. */
const hero = computed(() =>
  ratings.value.reduce((a, b) => ((b.games ?? 0) > (a.games ?? 0) ? b : a), ratings.value[0]!),
);
const others = computed(() => ratings.value.filter((r) => r !== hero.value));
</script>

<template>
  <div v-if="ratings.length" class="head" role="group" aria-label="Lichess ratings">
    <UiStat :label="hero.label" :value="hero.value" :hint="hero.hint" size="xl" />
    <UiStats v-if="others.length" :columns="others.length" dense class="rest">
      <UiStat v-for="r in others" :key="r.speed" :label="r.label" :value="r.value" :hint="r.hint" size="md" />
    </UiStats>
  </div>
  <UiEmpty v-else dense title="No Lichess ratings" />
</template>

<style scoped>
.head {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--sp-3) var(--sp-5);
}

.rest {
  flex: 1 1 14rem;
  max-width: 24rem;
}
</style>
