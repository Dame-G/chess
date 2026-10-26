<script setup lang="ts">
import { computed, watchEffect } from "vue";
import { RouterView, useRoute } from "vue-router";
import { EAppFooter } from "@eos-ui/vue/components/EAppFooter";
import { EAppShell } from "@eos-ui/vue/components/EAppShell";
import { EToastProvider } from "@eos-ui/vue/components/EToastProvider";
import { useBoardAddress } from "./board/address";
import { useBoard } from "./board/board";
import BoardStage from "./board/BoardStage.vue";
import { seatName } from "./format";
import Panel from "./panel/Panel.vue";
import GamesRail from "./rail/GamesRail.vue";
import CommandBar from "./shell/CommandBar.vue";
import { useKeys } from "./shell/keys";
import NewGame from "./shell/NewGame.vue";
import Shortcuts from "./shell/Shortcuts.vue";
import StatusLine from "./shell/StatusLine.vue";
import { useEventToasts } from "./shell/toasts";
import TopBar from "./shell/TopBar.vue";

/**
 * The app is the board plus a panel, with the Games rail beside them; profiles and settings are sheets over the
 * board's right side (the whole window below 75rem), never instead of it.
 */
const route = useRoute();
const board = useBoard();
const sheet = computed(() => route.meta["sheet"] === true);

useBoardAddress();
useKeys();
useEventToasts();

watchEffect(() => {
  const s = board.summary;
  const place = route.meta["title"] as string | undefined;
  const game = s
    ? `${s.you === "white" ? "You" : seatName(s.white)} – ${s.you === "black" ? "You" : seatName(s.black)}`
    : "";
  const what = place || game;
  document.title = what ? `${what} · Dameless` : "Dameless";
});
</script>

<template>
  <EToastProvider>
    <EAppShell class="shell">
      <template #header>
        <TopBar />
      </template>

      <div class="ws" :class="{ 'ws--sheet': sheet }">
        <GamesRail class="ws-rail" />
        <BoardStage class="ws-stage" />
        <Panel class="ws-panel" />
        <div v-if="sheet" class="ws-sheet">
          <RouterView />
        </div>
      </div>

      <template #footer>
        <EAppFooter class="foot">
          <StatusLine />
        </EAppFooter>
      </template>
    </EAppShell>
    <CommandBar />
    <Shortcuts />
    <NewGame />
  </EToastProvider>
</template>

<style scoped>
/* The workspace fills the window between the header and the status line; panes scroll on their own. */
.shell {
  --e-shell-width: 100%;

  grid-template-columns: minmax(0, 1fr);
  height: 100dvh;
  min-height: 0;
}

@media (pointer: fine) {
  .shell {
    --e-control-height: 2.5rem;
    --e-tap: 2.25rem;
  }
}

.shell :deep(.e-shell-body) {
  min-width: 0;
  min-height: 0;
  padding: 0;
}

.shell :deep(.e-shell-main) {
  min-width: 0;
  min-height: 0;
}

.ws {
  --ws-pad: 0.75rem;
  --line: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);

  position: relative;
  display: grid;
  grid-template-columns: 18rem minmax(0, 1fr) minmax(21rem, 27rem);
  height: 100%;
  min-height: 0;
}

.ws-rail {
  border-inline-end: var(--line);
  background: var(--e-surface);
}

.ws-stage {
  min-height: 0;
}

.ws-panel {
  border-inline-start: var(--line);
  background: var(--e-surface);
}

/* A sheet covers the panel and part of the board's right side; the board stays visible and live. */
.ws-sheet {
  position: absolute;
  inset-block: 0;
  inset-inline-end: 0;
  z-index: 5;
  inline-size: min(40rem, calc(100% - 18rem - 24rem));
  min-inline-size: 30rem;
  border-inline-start: var(--line);
  box-shadow: -0.5rem 0 1.5rem -1rem rgb(0 0 0 / 35%);
  animation: sheet-in var(--e-duration-move, 0.18s) ease-out both;
}

@keyframes sheet-in {
  from {
    opacity: 0;
    transform: translateX(1.5rem);
  }
}

@media (prefers-reduced-motion: reduce) {
  .ws-sheet {
    animation: none;
  }
}

.foot :deep(.e-appfooter-inner),
.foot {
  padding-block: 0;
}

/* Stacked below 75rem: the board first, then the panel, then the rail; the page scrolls. Sheets cover the window. */
@media (width < 75rem) {
  .shell {
    height: auto;
    min-height: 100dvh;
  }

  .ws {
    grid-template-columns: minmax(0, 1fr);
    height: auto;
  }

  .ws-stage {
    order: 1;
  }

  .ws-panel {
    order: 2;
    border-block-start: var(--line);
    border-inline-start: 0;
  }

  .ws-rail {
    order: 3;
    height: 28rem;
    border-block-start: var(--line);
    border-inline-end: 0;
  }

  .ws-sheet {
    position: fixed;
    inset: 0;
    z-index: 20;
    inline-size: auto;
    min-inline-size: 0;
    border: 0;
    box-shadow: none;
  }
}
</style>
