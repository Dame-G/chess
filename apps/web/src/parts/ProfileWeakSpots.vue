<script setup lang="ts">
import { ref } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import type { Profile, WeakSpot } from "../api/types";
import { readClock } from "../shell/commands";
import { playOutSeat } from "../sheets/levels";
import { useDameless } from "../stores/dameless";
import { useGames } from "../stores/games";
import { stored } from "../stored";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSection from "../ui/UiSection.vue";

/**
 * Where your games went wrong: the positions before your mistakes, the themes that come back first, each one a way to
 * play the position out again against Dameless at about your level.
 */
defineProps<{ profile: Profile }>();

const router = useRouter();
const toast = useToast();
const games = useGames();
const dameless = useDameless();
const clock = stored("new.clock", "5+3");

const starting = ref<string | null>(null);
/** Starts a local game against Dameless from the position before the mistake, with You to move. */
async function playOut(spot: WeakSpot) {
  const key = `${spot.game}:${spot.ply}`;
  starting.value = key;
  const whiteToMove = spot.fen.split(" ")[1] !== "b";
  try {
    // At the level nearest your rating, so the position is practised against someone you can learn from.
    if (!dameless.levels) await dameless.loadLevels().catch(() => {});
    const seat = playOutSeat(dameless.levels);
    const s = await games.start({
      white: whiteToMove ? "you" : seat,
      black: whiteToMove ? seat : "you",
      time: readClock(clock.value) ?? { type: "clock", initial_ms: 300_000, increment_ms: 3000 },
      from: { type: "position", fen: spot.fen, moves: [] },
    });
    await router.push({ path: "/", query: { game: String(s.id) } });
  } catch (e) {
    toast.show({
      title: "The game did not start",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  } finally {
    starting.value = null;
  }
}

const boardTo = (spot: WeakSpot) => ({ path: "/", query: { game: String(spot.game), ply: String(spot.ply) } });
</script>

<template>
  <UiSection title="Weak spots">
    <template #note>Positions before your mistakes, themes that come back first</template>
    <ul v-if="profile.weak_spots.length" class="spots">
      <li v-for="s in profile.weak_spots" :key="`${s.game}-${s.ply}`" class="spot">
        <div class="text">
          <p class="line">
            <span class="mono played">{{ Math.ceil(s.ply / 2) }}{{ s.ply % 2 ? "." : "…" }} {{ s.played }}</span>
            <span class="better">
              better <span class="mono">{{ s.best }}</span>
            </span>
            <span class="swing num">−{{ s.swing.toFixed(0) }}%</span>
          </p>
          <p class="meta">
            {{ s.opening ?? "Unnamed opening" }}, {{ s.phase
            }}<template v-if="s.recurring > 1"> · comes up {{ s.recurring }} times</template> ·
            <ELink :link-component="RouterLink" :to="boardTo(s)">see the game</ELink>
          </p>
        </div>
        <EButton variant="outline" size="sm" :loading="starting === `${s.game}:${s.ply}`" @click="playOut(s)">
          Play this out
        </EButton>
      </li>
    </ul>
    <UiEmpty v-else dense title="No weak spots yet. They come from the mistakes in your reviewed games." />
  </UiSection>
</template>

<style scoped>
.spots {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.spot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-3);
  min-height: var(--h-row-2);
  padding-block: var(--sp-2);
}

.spot + .spot {
  border-block-start: var(--e-stroke-hair) solid var(--c-line);
}

.text {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
}

.line {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0 var(--sp-3);
  margin: 0;
}

.played {
  font-weight: 600;
}

.better {
  color: var(--c-ink-2);
}

.swing {
  color: var(--rv-blunder);
  font: var(--t-fig);
}

.meta {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
