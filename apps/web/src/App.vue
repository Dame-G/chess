<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, watch, watchEffect } from "vue";
import { RouterView, useRoute } from "vue-router";
import { EAppShell } from "@eos-ui/vue/components/EAppShell";
import { EToastProvider } from "@eos-ui/vue/components/EToastProvider";
import { useBoardAddress } from "./board/address";
import { reachable } from "./api/stream";
import { useBoard } from "./board/board";
import BoardStage from "./board/BoardStage.vue";
import { seatName } from "./format";
import { down, up, useMedia } from "./media";
import Panel from "./panel/Panel.vue";
import GamesRail from "./rail/GamesRail.vue";
import CommandBar from "./shell/CommandBar.vue";
import { useKeys } from "./shell/keys";
import NewGame from "./shell/NewGame.vue";
import UiConfirm from "./ui/UiConfirm.vue";
import Shortcuts from "./shell/Shortcuts.vue";
import TabBar from "./shell/TabBar.vue";
import { useEventToasts, useLoginResult } from "./shell/toasts";
import TopBar from "./shell/TopBar.vue";
import { useUi } from "./shell/ui";

/**
 * The app is the board plus a panel, with the Games rail beside them; profiles and settings are sheets in the
 * panel's place beside the board, never instead of it. Below 75rem the rail is a drawer from the header and a sheet
 * covers the window; on a phone the board and its panel are one page and a bar at the foot moves between Board,
 * Games, You and Dameless (docs/ui-brief.md, 4.1 and 4.5).
 */
const route = useRoute();
const board = useBoard();
const ui = useUi();
const sheet = computed(() => route.meta["sheet"] === true);
const phone = useMedia(down("md"));
const wide = useMedia(up("lg"));

useBoardAddress();
useKeys();
useEventToasts();
useLoginResult();

// The Games rail is a place you go to pick a game or a sheet: once you have, it steps aside.
watch(sheet, (open) => {
  if (open) ui.railOpen = false;
});
watch(
  () => board.gameId,
  () => (ui.railOpen = false),
);
watch(wide, (roomy) => {
  if (roomy) ui.railOpen = false;
});

/** Escape closes the drawer (it does not take focus, so it does not hear the key itself). */
function onKey(e: KeyboardEvent) {
  if (e.key === "Escape" && ui.railOpen && !wide.value && !phone.value && !document.querySelector('[role="dialog"]')) {
    ui.railOpen = false;
  }
}
onMounted(() => document.addEventListener("keydown", onKey));
onBeforeUnmount(() => document.removeEventListener("keydown", onKey));

watchEffect(() => {
  const s = board.summary;
  const place = route.meta["title"] as string | undefined;
  const game = s
    ? `${s.you === "white" ? "You" : seatName(s.white)} vs ${s.you === "black" ? "You" : seatName(s.black)}`
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

      <div class="ws" :class="{ 'ws--sheet': sheet, 'ws--games': ui.railOpen }">
        <GamesRail class="ws-rail" :class="{ 'ws-rail--open': ui.railOpen }" />
        <button
          v-if="ui.railOpen && !wide && !phone"
          type="button"
          class="scrim"
          aria-label="Close games"
          tabindex="-1"
          @click="ui.railOpen = false"
        ></button>
        <BoardStage class="ws-stage" />
        <Panel class="ws-panel" />
        <RouterView v-if="sheet" />
        <p v-if="!reachable" class="offline" role="status">Can't reach the app. Reconnecting…</p>
      </div>
      <TabBar v-if="phone" />
    </EAppShell>
    <CommandBar />
    <Shortcuts />
    <NewGame />
    <UiConfirm />
  </EToastProvider>
</template>

<style scoped>
/* The workspace fills the window under the header; the panes scroll on their own. */
.shell {
  /* One gutter for the header and the panes, so their edges line up (eos's own is 36px). */
  --e-shell-width: 100%;
  --e-gutter: var(--sp-3);

  /* What a sheet that covers the window leaves visible: the header above it and, on a phone, the tab bar below. */
  --h-tabbar: 3.5rem;

  grid-template-columns: minmax(0, 1fr);
  height: 100dvh;
  min-height: 0;
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
  /* Until the panes and sheets that still read them use the kit's names (docs/ui-brief.md, 2.1 and 2.5). */
  --ws-pad: var(--sp-3);
  --line: var(--e-stroke-hair) solid var(--c-line);

  position: relative;
  display: grid;
  grid-template-columns: var(--w-rail) minmax(0, 1fr) var(--w-panel);
  height: 100%;
  min-height: 0;
}

.ws-rail {
  border-inline-end: var(--line);
  background: var(--c-pane);
}

.ws-stage {
  min-height: 0;
}

.ws-panel {
  min-height: 0;
  overflow: hidden auto;
  border-inline-start: var(--line);
  background: var(--c-pane);
}

/* A sheet takes the panel's place beside the board, which shrinks to fit and stays visible and live: the sheet docks
   over the column kept for it, as wide as the sheet is. */
.ws--sheet {
  grid-template-columns: var(--w-rail) minmax(0, 1fr) var(--w-sheet);
}

.ws--sheet:has(.ui-sheet--wide) {
  grid-template-columns: var(--w-rail) minmax(0, 1fr) var(--w-sheet-wide);
}

.ws--sheet .ws-panel {
  display: none;
}

/* One notice for a lost connection, over the top of the workspace, instead of an error in every pane. */
.offline {
  position: absolute;
  inset: 0 0 auto;
  z-index: var(--e-z-sticky);
  margin: 0;
  padding: var(--sp-1) var(--sp-3);
  background: var(--c-warn-wash);
  color: var(--c-ink);
  font: var(--t-meta);
  text-align: center;
}

.scrim {
  display: none;
}

/* A tablet (48rem to 75rem): the board on the left, sized by its height, and the panel; the rail is a drawer. A sheet
   covers the window below the header. */
@media (width < 75rem) {
  .shell {
    --sheet-inset-top: calc(var(--h-header) + var(--e-stroke-hair));
  }

  .ws,
  .ws--sheet,
  .ws--sheet:has(.ui-sheet--wide) {
    grid-template-columns: minmax(0, 1fr) min(var(--w-panel), 46vw);
  }

  .ws-rail {
    position: absolute;
    inset-block: 0;
    inset-inline-start: 0;
    z-index: var(--e-z-focus);
    width: min(20rem, 86%);
    border-inline-end: var(--line);
    box-shadow: var(--e-shadow-float);
    transform: translateX(-101%);
    visibility: hidden;
    transition:
      transform var(--m-move) var(--m-ease),
      visibility 0s linear var(--m-move);
  }

  .ws-rail--open {
    transform: none;
    visibility: visible;
    transition-delay: 0s;
  }

  .scrim {
    position: absolute;
    inset: 0;
    z-index: var(--e-z-sticky);
    display: block;
    padding: 0;
    border: 0;
    background: color-mix(in oklab, var(--c-ink) 30%, transparent);
    cursor: default;
  }

  .ws-panel {
    grid-column: 2;
  }
}

@media (prefers-reduced-motion: reduce) {
  .ws-rail {
    transition: none;
  }
}

/* A phone (under 48rem): one page, the board edge to edge and its panel under it, scrolled as a page. The tab bar
   holds the foot of the window; Games is a screen of its own over the board. */
@media (width < 48rem) {
  .shell {
    --sheet-inset-bottom: calc(var(--h-tabbar) + env(safe-area-inset-bottom, 0px));

    height: auto;
    min-height: 100dvh;
  }

  .ws,
  .ws--sheet,
  .ws--sheet:has(.ui-sheet--wide) {
    grid-template-columns: minmax(0, 1fr);
    height: auto;
    padding-block-end: calc(var(--h-tabbar) + env(safe-area-inset-bottom, 0px));
  }

  .ws-stage {
    order: 1;
  }

  .ws-panel {
    grid-column: 1;
    order: 2;
    border-block-start: var(--line);
    border-inline-start: 0;
  }

  .ws-rail {
    position: static;
    display: none;
    width: auto;
    border-inline-end: 0;
    box-shadow: none;
    transform: none;
    visibility: visible;
  }

  .ws--games {
    height: calc(100dvh - var(--sheet-inset-top));
    padding-block-end: 0;
  }

  .ws--games .ws-rail {
    display: block;
    height: calc(100dvh - var(--sheet-inset-top) - var(--sheet-inset-bottom));
  }

  .ws--games .ws-stage,
  .ws--games .ws-panel {
    display: none;
  }
}
</style>
