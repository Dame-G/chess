<script setup lang="ts">
import { computed, onMounted, watch } from "vue";
import { RouterLink, useRoute } from "vue-router";
import { PhBarbell, PhGearSix, PhListBullets, PhMagnifyingGlass, PhPlus } from "@phosphor-icons/vue";
import { EAppHeader } from "@eos-ui/vue/components/EAppHeader";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EKbd } from "@eos-ui/vue/components/EKbd";
import { formatShortcut } from "@eos-ui/vue";
import { down, up, useMedia } from "../media";
import { useConnections } from "../stores/connections";
import { useDrills } from "../stores/drills";
import UiPlayer from "../ui/UiPlayer.vue";
import Logo from "./Logo.vue";
import LiveGame from "./LiveGame.vue";
import { useStartDrills } from "./drillFlow";
import { useUi } from "./ui";
import WorkIndicator from "./WorkIndicator.vue";

/**
 * The header: the wordmark, the command bar, New game, Train with the drills waiting, You as a chip that opens your
 * sheet, the background work, settings. On a tablet the Games rail is a button here; on a phone the chip lives in the
 * tab bar and the header keeps the icons.
 */
const ui = useUi();
const drills = useDrills();
const startDrills = useStartDrills();

/** Train is offered once there are drills, and not while you play on Lichess. */
const training = computed(() => drills.available && drills.total > 0);
const trainLabel = computed(() => (drills.waiting ? `Train: ${drills.waiting} drills waiting` : "Train"));
const readToday = () => {
  if (drills.available && !drills.loaded) void drills.loadToday().catch(() => {});
};
onMounted(readToday);
watch(() => drills.available, readToday);
const route = useRoute();
const connections = useConnections();

const keys = formatShortcut("mod+k");

const phone = useMedia(down("md"));
const wide = useMedia(up("lg"));

const you = computed(() => ({ name: "You", key: "you", rating: connections.youRating }));
const onYou = computed(() => route.path === "/you");
</script>

<template>
  <EAppHeader class="top" brand-to="/" brand-label="Chess, the board" :link-component="RouterLink" :sticky="phone">
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
      <EButton size="sm" variant="outline" :icon="PhPlus" class="new" aria-label="New game" @click="ui.startNewGame()">
        <span class="new-label">New game</span>
      </EButton>
      <EButton
        v-if="training"
        size="sm"
        variant="quiet"
        :icon="PhBarbell"
        class="train"
        :aria-label="trainLabel"
        :title="`${trainLabel} (D)`"
        @click="startDrills()"
      >
        <span class="train-label">Train</span>
        <span v-if="drills.waiting" class="train-count num">{{ drills.waiting }}</span>
      </EButton>
      <UiPlayer v-if="!phone" :seat="you" size="md" link :current="onYou" title="Your profile" class="chip" />
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
  min-inline-size: 0;
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

/* Train reads "Train · 12": the count is a figure, after the word. */
.train-count {
  display: inline-block;
  text-decoration: none;
  margin-inline-start: var(--sp-1);
  color: var(--c-ink-2);
  font: var(--t-fig);
}

.train-count::before {
  content: "· ";
}

/* On a phone the buttons are their icons; Train keeps its count. */
@media (width < 48rem) {
  .new-label,
  .train-label {
    display: none;
  }

  .train-count::before {
    content: none;
  }
}
</style>
