<script setup lang="ts">
import type { BadgeTone } from "./badge";

/**
 * A status as a word: "Running", "Kept", "Connected", "BOT". Sentence case, tinted, no border. `solid` is for LIVE and
 * nothing else; `dot` puts the tone's own dot before the word.
 */
withDefaults(defineProps<{ tone?: BadgeTone; solid?: boolean; dot?: boolean }>(), { tone: "neutral" });
</script>

<template>
  <span class="ui-badge" :class="[`ui-badge--${tone}`, { 'ui-badge--solid': solid }]">
    <span v-if="dot" class="ui-badge-dot" aria-hidden="true" />
    <slot />
  </span>
</template>

<style scoped>
.ui-badge {
  --_tone: var(--c-ink-2);
  --_wash: var(--c-neutral-wash);
  --_on: var(--e-surface);

  display: inline-flex;
  flex: none;
  align-items: center;
  gap: var(--sp-1);
  box-sizing: border-box;
  height: var(--h-badge);
  padding-inline: calc(var(--sp-1) * 1.5);
  border-radius: var(--r-1);
  background: var(--_wash);
  color: var(--_tone);
  font: 600 var(--e-text-xs) / 1 var(--e-font-sans);
  white-space: nowrap;
}

.ui-badge--win {
  --_tone: var(--c-win);
  --_wash: var(--c-win-wash);
}

.ui-badge--loss {
  --_tone: var(--c-loss);
  --_wash: var(--c-loss-wash);
}

.ui-badge--warn {
  --_tone: var(--c-warn);
  --_wash: var(--c-warn-wash);
}

.ui-badge--info {
  --_tone: var(--c-info);
  --_wash: var(--c-info-wash);
}

.ui-badge--act {
  --_tone: var(--c-act);
  --_wash: var(--c-act-wash);
  --_on: var(--c-act-ink);
}

/* Solid: the tone is the fill and the pane's own colour is the text. */
.ui-badge--solid {
  background: var(--_tone);
  color: var(--_on);
}

.ui-badge-dot {
  width: var(--dot-sm);
  height: var(--dot-sm);
  border-radius: var(--r-pill);
  background: currentcolor;
}
</style>
