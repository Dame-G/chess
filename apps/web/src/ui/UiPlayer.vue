<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import { formatRating } from "../format";
import { playerKey, playerName, playerPath, type PlayerLike } from "./player";
import UiBadge from "./UiBadge.vue";
import UiLive from "./UiLive.vue";

/**
 * The one way a player is named: the side they play as a small square, their name, a title (`BOT`, `FM`) as a badge
 * and a rating in muted figures, with `?` only while it is provisional. `md` is a chip for the header; `link` makes it
 * open their page; `live` puts the live dot before the name.
 */
const props = withDefaults(
  defineProps<{
    seat: PlayerLike;
    /** Overrides the seat's rating; `null` shows none. */
    rating?: number | string | null;
    provisional?: boolean;
    colour?: "white" | "black";
    size?: "sm" | "md";
    link?: boolean;
    live?: boolean;
    /** The chip of the page you are on (a quiet fill, not the action colour). */
    current?: boolean;
    title?: string;
  }>(),
  { size: "sm", rating: undefined },
);

const name = computed(() => playerName(props.seat));
const key = computed(() => playerKey(props.seat));
const to = computed(() => (props.link && key.value ? playerPath(key.value) : null));
const badge = computed(() => props.seat.title ?? null);
const shown = computed(() => (props.rating !== undefined ? props.rating : (props.seat.rating ?? null)));
const rated = computed(() =>
  shown.value == null || shown.value === "" ? null : formatRating(shown.value, props.provisional),
);
</script>

<template>
  <component
    :is="to ? RouterLink : 'span'"
    :to="to ?? undefined"
    class="ui-player"
    :class="[`ui-player--${size}`, { 'is-link': to, 'is-current': current }]"
    :title="title ?? name"
    :aria-current="current ? 'page' : undefined"
  >
    <span
      v-if="colour"
      class="ui-player-pip"
      :class="`ui-player-pip--${colour}`"
      role="img"
      :aria-label="`plays ${colour}`"
    />
    <UiLive v-if="live" label="Live" />
    <span class="ui-player-name truncate">{{ name }}</span>
    <UiBadge v-if="badge">{{ badge }}</UiBadge>
    <span v-if="rated" class="ui-player-rating num">{{ rated }}</span>
  </component>
</template>

<style scoped>
.ui-player {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  box-sizing: border-box;
  min-width: 0;
  max-width: 100%;
  color: var(--c-ink);
  font: var(--t-body-strong);
  text-decoration: none;
}

.ui-player-name {
  min-width: 0;
}

.ui-player-rating {
  flex: none;
  color: var(--c-ink-2);
  font: var(--t-fig);
}

.ui-player-pip {
  flex: none;
  box-sizing: border-box;
  width: var(--dot-lg);
  height: var(--dot-lg);
  border: var(--e-stroke-hair) solid var(--c-line-strong);
  border-radius: var(--r-0);
}

.ui-player-pip--white {
  background: var(--c-pip-white);
}

.ui-player-pip--black {
  background: var(--c-pip-black);
}

/* The header chip: the same shape whether or not it is the page you are on. */
.ui-player--md {
  min-height: var(--h-control);
  padding-inline: var(--sp-2);
  border-radius: var(--r-1);
}

.ui-player--md.is-current {
  background: var(--c-sunk);
}

.ui-player.is-link:hover {
  background: var(--c-raise);
}

.ui-player.is-link:hover .ui-player-name {
  text-decoration: underline;
  text-decoration-color: var(--c-line-strong);
  text-underline-offset: 0.2em;
}

.ui-player.is-link:focus-visible {
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}
</style>
