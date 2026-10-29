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
import UiConfirm from "./ui/UiConfirm.vue";
import Shortcuts from "./shell/Shortcuts.vue";
import StatusLine from "./shell/StatusLine.vue";
import { useEventToasts, useLoginResult } from "./shell/toasts";
import TopBar from "./shell/TopBar.vue";

/**
 * The app is the board plus a panel, with the Games rail beside them; profiles and settings are sheets in the
 * panel's place beside the board (the whole window below 75rem), never instead of it.
 */
const route = useRoute();
const board = useBoard();
const sheet = computed(() => route.meta["sheet"] === true);

useBoardAddress();
useKeys();
useEventToasts();
useLoginResult();

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
        <RouterView v-if="sheet" />
      </div>

      <template #footer>
        <EAppFooter class="shell-foot">
          <StatusLine />
        </EAppFooter>
      </template>
    </EAppShell>
    <CommandBar />
    <Shortcuts />
    <NewGame />
    <UiConfirm />
  </EToastProvider>
</template>

<style scoped>
/* The workspace fills the window between the header and the status line; panes scroll on their own. */
.shell {
  /* One gutter for the header, the panes and the status line, so their edges line up (eos's own is 36px). */
  --e-shell-width: 100%;
  --e-gutter: 0.75rem;

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
  --ws-rail: clamp(18rem, 20vw - 4rem, 22rem);

  /* The panel yields to the board: it is as narrow as it can read at the narrowest desktop and grows with the window. */
  --ws-panel: clamp(21rem, 26vw, 30rem);
  --line: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);

  position: relative;
  display: grid;
  grid-template-columns: var(--ws-rail) minmax(0, 1fr) var(--ws-panel);
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

/* A sheet takes the panel's place beside the board, which shrinks to fit and stays visible and live: the sheet docks
   over the column kept for it. */
.ws--sheet {
  grid-template-columns: var(--ws-rail) minmax(0, 1fr) var(--e-panel-width-md);
}

.ws--sheet .ws-panel {
  display: none;
}

.shell-foot :deep(.e-appfooter-inner),
.shell-foot {
  padding-block: 0;
}

/* Stacked below 75rem: the board first, then the panel, then the rail; the page scrolls.
   Sheets cover the window (ESheet's modalBelow). */
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

  /* The status line moves into the header (a compact indicator), so the page does not end on it. */
  .shell-foot {
    display: none;
  }

  .ws-rail {
    order: 3;
    height: 28rem;
    border-block-start: var(--line);
    border-inline-end: 0;
  }

  .ws--sheet {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
