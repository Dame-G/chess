<script setup lang="ts">
import { computed } from "vue";
import { RouterLink, useRoute } from "vue-router";
import { PhGearSix, PhMagnifyingGlass, PhPlus } from "@phosphor-icons/vue";
import { EAppHeader } from "@eos-ui/vue/components/EAppHeader";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EKbd } from "@eos-ui/vue/components/EKbd";
import { EStatLink } from "@eos-ui/vue/components/EStatLink";
import { formatShortcut } from "@eos-ui/vue";
import { isLive } from "../board/modes";
import { useMedia } from "../media";
import { useConnections } from "../stores/connections";
import { useGames } from "../stores/games";
import { useVersions } from "../stores/versions";
import Logo from "./Logo.vue";
import StatusLine from "./StatusLine.vue";
import { useUi } from "./ui";

/** The header: the wordmark, the command bar, You and Dameless as links to their sheets, New game, settings. */
const ui = useUi();
const route = useRoute();
const connections = useConnections();
const versions = useVersions();
const games = useGames();

/** New game is the header's one filled button, while You have no game running. */
const playing = computed(() => games.list.some((g) => g.you && isLive(g)));
const keys = formatShortcut("mod+k");

/** On a phone the links keep their names and drop their figures (rating, version); the sheets show them. */
const tight = useMedia("(width < 30rem)");
</script>

<template>
  <EAppHeader class="top" brand-to="/" brand-label="Dameless, the board" :link-component="RouterLink">
    <template #brand><Logo /></template>
    <template #logo>
      <EButton
        variant="outline"
        size="sm"
        :icon="PhMagnifyingGlass"
        class="bar"
        aria-label="Search or run a command"
        @click="ui.palette = true"
      >
        <span class="bar-label">Search or run a command</span>
        <span class="bar-keys"
          ><EKbd v-for="k in keys" :key="k" size="sm">{{ k }}</EKbd></span
        >
      </EButton>
    </template>
    <template #actions>
      <EButton v-if="!playing" size="sm" :icon="PhPlus" class="new" aria-label="New game" @click="ui.startNewGame()">
        <template #default><span class="new-label">New game</span></template>
      </EButton>
      <StatusLine compact class="work-compact" />
      <EStatLink
        label="You"
        :value="tight ? undefined : connections.youRating || undefined"
        to="/you"
        :link-component="RouterLink"
        :current="route.path === '/you'"
      />
      <EStatLink
        label="Dameless"
        :value="tight ? undefined : versions.current?.name"
        to="/dameless"
        :link-component="RouterLink"
        :current="route.path.startsWith('/dameless')"
      />
      <EIconButton
        :icon="PhGearSix"
        label="Settings"
        tooltip
        size="sm"
        to="/settings"
        :link-component="RouterLink"
        :current="route.path === '/settings'"
      />
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

/* The wordmark stays at the left edge; the command bar centres in the room between it and the actions. */
.top :deep(.e-appheader-logo) {
  flex: 1 1 0;
  justify-content: flex-start;
  min-inline-size: 0;
}

.bar {
  justify-content: flex-start;
  margin-inline: auto;
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

/* Stacked (below 75rem) the work indicator moves up here from the status line at the foot. */
@media (width >= 75rem) {
  .top .work-compact {
    display: none;
  }
}

@media (width < 56rem) {
  .bar-label,
  .bar-keys {
    display: none;
  }

  .bar {
    inline-size: auto;
    margin-inline: 0;
  }

  .top :deep(.e-appheader-bar) {
    gap: var(--e-gap-snug);
  }

  .top :deep(.e-appheader-actions) {
    gap: 0;
  }
}

@media (width < 40rem) {
  .new-label {
    display: none;
  }
}
</style>
