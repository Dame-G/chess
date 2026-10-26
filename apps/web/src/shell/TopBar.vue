<script setup lang="ts">
import { computed } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { PhGearSix, PhMagnifyingGlass, PhPlus } from "@phosphor-icons/vue";
import { EAppHeader } from "@eos-ui/vue/components/EAppHeader";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EKbd } from "@eos-ui/vue/components/EKbd";
import { formatShortcut } from "@eos-ui/vue";
import { isLive } from "../board/modes";
import { useConnections } from "../stores/connections";
import { useGames } from "../stores/games";
import { useVersions } from "../stores/versions";
import Logo from "./Logo.vue";
import { useUi } from "./ui";

/** The header: the wordmark, the command bar, You and Dameless as links to their sheets, New game, settings. */
const ui = useUi();
const router = useRouter();
const connections = useConnections();
const versions = useVersions();
const games = useGames();

/** New game is the header's one filled button, while You have no game running. */
const playing = computed(() => games.list.some((g) => g.you && isLive(g)));
const keys = formatShortcut("mod+k");
</script>

<template>
  <EAppHeader class="top" brand-to="/" brand-label="Dameless, the board" :link-component="RouterLink">
    <template #brand><Logo /></template>
    <template #logo>
      <EButton variant="outline" size="sm" :icon="PhMagnifyingGlass" class="bar" @click="ui.palette = true">
        <span class="bar-label">Search or run a command</span>
        <span class="bar-keys"
          ><EKbd v-for="k in keys" :key="k" size="sm">{{ k }}</EKbd></span
        >
      </EButton>
    </template>
    <template #actions>
      <EButton v-if="!playing" size="sm" :icon="PhPlus" class="new" @click="ui.startNewGame()">New game</EButton>
      <RouterLink to="/you" class="who" aria-label="You: your profile">
        <span class="who-name">You</span>
        <span v-if="connections.youRating" class="who-figure num">{{ connections.youRating }}</span>
      </RouterLink>
      <RouterLink to="/dameless" class="who" aria-label="Dameless: the engine's profile">
        <span class="who-name">Dameless</span>
        <span v-if="versions.current" class="who-figure num">{{ versions.current.name }}</span>
      </RouterLink>
      <EIconButton :icon="PhGearSix" label="Settings" tooltip size="sm" @click="router.push('/settings')" />
    </template>
  </EAppHeader>
</template>

<style scoped>
.top {
  --e-control-height: 2.25rem;
  --e-tap: 2.25rem;
}

.top,
.top :deep(.e-appheader-bar) {
  min-inline-size: 0;
}

.top :deep(.e-appheader-bar) {
  min-block-size: 3rem;
  gap: var(--e-gap-base);
}

.top :deep(.e-appheader-logo) {
  flex: 1 1 0;
  justify-content: center;
  min-inline-size: 0;
}

.bar {
  justify-content: flex-start;
  inline-size: min(100%, 26rem);
  color: var(--e-muted);
  font-weight: 400;
}

.bar-label {
  flex: 1 1 auto;
  overflow: hidden;
  text-align: start;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bar-keys {
  display: inline-flex;
  gap: 0.125rem;
}

/* You and Dameless: a name and its one figure, a compact link to the sheet. */
.who {
  display: inline-flex;
  align-items: baseline;
  gap: 0.375rem;
  padding: 0.375rem 0.5rem;
  border-radius: var(--e-radius-sm);
  color: var(--e-ink);
  font-weight: 600;
  text-decoration: none;
  white-space: nowrap;
}

.who:hover {
  background: color-mix(in oklab, var(--e-ink) 6%, transparent);
}

.who.router-link-active {
  box-shadow: inset 0 -0.125rem 0 var(--e-primary);
  color: var(--e-primary);
}

.who-figure {
  color: var(--e-muted);
  font: 500 0.8125rem/1 var(--e-font-mono);
}

@media (width < 56rem) {
  .bar-label,
  .bar-keys {
    display: none;
  }

  .bar {
    inline-size: auto;
  }

  .top :deep(.e-appheader-logo) {
    justify-content: flex-start;
  }

  .top :deep(.e-appheader-bar) {
    gap: var(--e-gap-snug);
  }

  .top :deep(.e-appheader-actions) {
    gap: 0;
  }
}

@media (width < 40rem) {
  .who-figure {
    display: none;
  }

  .new :deep(.e-button-label) {
    display: none;
  }
}
</style>
