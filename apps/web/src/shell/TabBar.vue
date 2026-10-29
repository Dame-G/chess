<script setup lang="ts">
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import { PhCheckerboard, PhCpu, PhListBullets, PhUser } from "@phosphor-icons/vue";
import { useBoard } from "../board/board";
import { useUi } from "./ui";

/**
 * Navigation on a phone: Board, Games, You, Dameless. The Games rail is its own full screen; You and Dameless are
 * sheets, which stop short of this bar so it stays. The current tab has the action colour's bar under it. Settings
 * stays in the header, with the work indicator.
 */
const ui = useUi();
const route = useRoute();
const router = useRouter();
const board = useBoard();

type Tab = "board" | "games" | "you" | "dameless";
const current = computed<Tab | null>(() => {
  if (ui.railOpen) return "games";
  if (route.path === "/you") return "you";
  if (route.path.startsWith("/dameless")) return "dameless";
  return route.path === "/" ? "board" : null;
});

const tabs: { id: Tab; label: string; icon: typeof PhCheckerboard }[] = [
  { id: "board", label: "Board", icon: PhCheckerboard },
  { id: "games", label: "Games", icon: PhListBullets },
  { id: "you", label: "You", icon: PhUser },
  { id: "dameless", label: "Dameless", icon: PhCpu },
];

function go(id: Tab) {
  ui.railOpen = id === "games";
  if (id === "board" || id === "games") {
    // The board as it was: what is on it is its address, and the Games tab sits over it.
    if (route.path !== "/") void router.push({ path: "/", query: board.address });
  } else void router.push(id === "you" ? "/you" : "/dameless");
}
</script>

<template>
  <nav class="tabbar" aria-label="Main">
    <button
      v-for="t in tabs"
      :key="t.id"
      type="button"
      class="tab"
      :class="{ 'is-on': current === t.id }"
      :aria-current="current === t.id ? 'page' : undefined"
      @click="go(t.id)"
    >
      <component :is="t.icon" class="icon" :weight="current === t.id ? 'fill' : 'regular'" aria-hidden="true" />
      <span class="label">{{ t.label }}</span>
    </button>
  </nav>
</template>

<style scoped>
.tabbar {
  position: fixed;
  inset: auto 0 0;
  z-index: var(--e-z-sticky);
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  box-sizing: border-box;
  height: calc(var(--h-tabbar) + env(safe-area-inset-bottom, 0px));
  padding-block-end: env(safe-area-inset-bottom, 0px);
  border-block-start: var(--e-stroke-hair) solid var(--c-line);
  background: var(--c-pane);
}

.tab {
  position: relative;
  display: grid;
  place-content: center;
  justify-items: center;
  gap: var(--sp-1);
  min-width: 0;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--c-ink-2);
  cursor: pointer;
}

.tab.is-on {
  color: var(--c-ink);
}

/* The current tab's marker: the action colour, under its label. */
.tab.is-on::after {
  content: "";
  position: absolute;
  inset: auto var(--sp-4) 0;
  height: var(--e-stroke-focus);
  background: var(--c-act);
}

.tab:focus-visible {
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

.icon {
  font-size: var(--i-lg);
}

.label {
  font: var(--t-meta);
  font-weight: 500;
}

.tab.is-on .label {
  font-weight: 600;
}
</style>
