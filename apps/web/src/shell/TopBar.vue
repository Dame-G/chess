<script setup lang="ts">
import { computed } from "vue";
import { RouterLink, useRoute } from "vue-router";
import { PhGearSix, PhListBullets, PhMagnifyingGlass, PhPlus } from "@phosphor-icons/vue";
import { EAppHeader } from "@eos-ui/vue/components/EAppHeader";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EKbd } from "@eos-ui/vue/components/EKbd";
import { formatShortcut } from "@eos-ui/vue";
import { isLive } from "../board/modes";
import { down, up, useMedia } from "../media";
import { useConnections } from "../stores/connections";
import { useGames } from "../stores/games";
import { useVersions } from "../stores/versions";
import UiPlayer from "../ui/UiPlayer.vue";
import Logo from "./Logo.vue";
import LiveGame from "./LiveGame.vue";
import { useUi } from "./ui";
import WorkIndicator from "./WorkIndicator.vue";

/**
 * The header: the wordmark, the command bar, New game, You and Dameless as two chips of one shape that open their
 * sheets, the background work, settings. On a tablet the Games rail is a button here; on a phone the chips live in the
 * tab bar and the header keeps the icons.
 */
const ui = useUi();
const route = useRoute();
const connections = useConnections();
const versions = useVersions();
const games = useGames();

/** New game is the header's one filled button, until a game you sit in is running: then it is an outline, still there. */
const playing = computed(() => games.list.some((g) => g.you && isLive(g)));
const keys = formatShortcut("mod+k");

const phone = useMedia(down("md"));
const wide = useMedia(up("lg"));

const you = computed(() => ({ name: "You", key: "you", rating: connections.youRating }));
const dameless = computed(() => ({ name: "Dameless", key: "dameless" }));
const onYou = computed(() => route.path === "/you");
const onDameless = computed(() => route.path.startsWith("/dameless"));
</script>

<template>
  <EAppHeader class="top" brand-to="/" brand-label="Dameless, the board" :link-component="RouterLink" :sticky="phone">
    <template #brand><Logo /></template>
    <template #logo>
      <EIconButton
        v-if="!wide && !phone"
        :icon="PhListBullets"
        label="Games"
        tooltip
        size="sm"
        class="rail-toggle"
        :aria-expanded="ui.railOpen"
        @click="ui.railOpen = !ui.railOpen"
      />
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
      <LiveGame />
      <EButton
        size="sm"
        :variant="playing ? 'outline' : undefined"
        :icon="PhPlus"
        class="new"
        aria-label="New game"
        @click="ui.startNewGame()"
      >
        <span class="new-label">New game</span>
      </EButton>
      <template v-if="!phone">
        <UiPlayer :seat="you" size="md" link :current="onYou" title="Your profile" class="chip" />
        <UiPlayer
          :seat="dameless"
          :rating="versions.current?.name ?? null"
          size="md"
          link
          :current="onDameless"
          title="Dameless: ideas, strength, the bot"
          class="chip"
        />
      </template>
      <WorkIndicator />
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
.top,
.top :deep(.e-appheader-bar) {
  min-inline-size: 0;
}

.top :deep(.e-appheader-bar) {
  min-block-size: var(--h-header);
  gap: var(--sp-3);
}

/* The wordmark stays at the left edge; the command bar centres in the room between it and the actions. */
.top :deep(.e-appheader-logo) {
  display: flex;
  flex: 1 1 0;
  align-items: center;
  justify-content: flex-start;
  gap: var(--sp-2);
  min-inline-size: 0;
}

.top :deep(.e-appheader-actions) {
  gap: var(--sp-2);
}

.bar {
  justify-content: flex-start;
  margin-inline: auto;
  inline-size: min(100%, 26rem);
  color: var(--c-ink-2);
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
  gap: var(--sp-1);
}

/* The two chips are one shape: the page you are on has the wash, never a different outline. */
.chip {
  flex: none;
}

/* A tablet: the command bar is its icon, which leaves room for the chips. */
@media (width < 75rem) {
  .bar-label,
  .bar-keys {
    display: none;
  }

  .bar {
    inline-size: auto;
    margin-inline: 0;
  }
}

/* A phone: the wordmark is its tile, the actions are icons. */
@media (width < 48rem) {
  .top :deep(.e-appheader-bar) {
    gap: var(--sp-2);
  }

  .top :deep(.e-appheader-actions) {
    gap: var(--sp-1);
  }
}

/* On a phone the button is its plus. */
@media (width < 48rem) {
  .new-label {
    display: none;
  }
}
</style>
