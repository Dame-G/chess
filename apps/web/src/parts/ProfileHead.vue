<script setup lang="ts">
import { computed } from "vue";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ETag } from "@eos-ui/vue/components/ETag";
import type { Player } from "../api/types";
import { speedName } from "../format";

/** Who a profile is about: the name, the title, and the Lichess ratings per speed. */
const props = defineProps<{ player: Player; games?: number }>();

const ratings = computed(() =>
  (["bullet", "blitz", "rapid", "classical"] as const).flatMap((s) => {
    const r = props.player.ratings[s];
    return r ? [{ speed: s, label: speedName[s], rating: r.rating, provisional: r.provisional, games: r.games }] : [];
  }),
);
const profileUrl = computed(() =>
  props.player.lichess && props.player.key !== "dameless" ? `https://lichess.org/@/${props.player.lichess}` : null,
);
</script>

<template>
  <header class="head">
    <div class="who">
      <h2 class="name">{{ player.name }}</h2>
      <ETag v-if="player.title" :label="player.title" size="sm" />
      <ELink v-if="profileUrl" :href="profileUrl" external class="out">On Lichess</ELink>
    </div>
    <ul v-if="ratings.length" class="plain ratings" aria-label="Lichess ratings">
      <li v-for="r in ratings" :key="r.speed" class="rating">
        <span class="rating-speed">{{ r.label }}</span>
        <span class="rating-value mono">{{ r.rating }}{{ r.provisional ? "?" : "" }}</span>
      </li>
    </ul>
    <p v-else class="sec-note">No Lichess ratings.</p>
  </header>
</template>

<style scoped>
.head {
  display: grid;
  gap: var(--e-gap-snug);
  padding-block-end: var(--e-gap-base);
}

.who {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug);
}

.name {
  margin: 0;
  font: var(--e-type-h4, 600 1.125rem/1.2 var(--e-font-sans));
}

.out {
  margin-inline-start: auto;
  font: var(--e-type-small);
}

.ratings {
  grid-auto-flow: column;
  grid-auto-columns: max-content;
  gap: var(--e-gap-base);
  overflow-x: auto;
}

.rating {
  display: grid;
}

.rating-speed {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.rating-value {
  font-size: 1.125rem;
  font-weight: 600;
}
</style>
