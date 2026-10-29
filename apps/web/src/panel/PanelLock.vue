<script setup lang="ts">
import { computed } from "vue";
import { useRouter } from "vue-router";
import { PhLockSimple } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { isLive, onLichessAsYou } from "../board/modes";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import UiEmpty from "../ui/UiEmpty.vue";

/**
 * The fair-play lock: while You play on Lichess no analysis is on screen, and a panel that would show some says so in
 * one sentence (the server's own) and offers the way back to the game. The same in every mode.
 */
const policy = usePolicy();
const games = useGames();
const router = useRouter();

const mine = computed(() => games.list.find((g) => isLive(g) && onLichessAsYou(g)));

function back() {
  if (mine.value) void router.push({ path: "/", query: { game: String(mine.value.id) } });
}
</script>

<template>
  <UiEmpty :icon="PhLockSimple" :title="policy.reason ?? 'Analysis is off while you play on Lichess'">
    <template v-if="mine" #actions>
      <EButton variant="outline" @click="back">Back to my game</EButton>
    </template>
  </UiEmpty>
</template>
