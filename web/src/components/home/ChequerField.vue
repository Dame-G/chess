<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";

// A flat chequerboard drifting slowly along one diagonal, faded to nothing at the edges. It is pure CSS: the board is a
// tiled gradient on a layer one period larger than the window, and the layer slides by exactly one period per loop, so
// the loop has no seam and nothing pops in or out. Only a transform animates (on the compositor). It takes no input.
// The script only pauses the animation while the field is off screen or the tab is hidden.

const root = ref<HTMLElement | null>(null);
const paused = ref(false);

let cleanup: Array<() => void> = [];

onMounted(() => {
  const el = root.value;
  if (!el) return;
  let onScreen = true;
  const sync = () => {
    paused.value = !onScreen || document.visibilityState === "hidden";
  };
  const io = new IntersectionObserver((entries) => {
    onScreen = entries.some((entry) => entry.isIntersecting);
    sync();
  });
  io.observe(el);
  document.addEventListener("visibilitychange", sync);
  cleanup.push(() => {
    io.disconnect();
    document.removeEventListener("visibilitychange", sync);
  });
});

onBeforeUnmount(() => {
  cleanup.forEach((fn) => fn());
  cleanup = [];
});
</script>

<template>
  <div ref="root" class="chequer" :class="{ paused }" aria-hidden="true">
    <div class="chequer-board" />
  </div>
</template>

<style scoped>
.chequer {
  /* One square; the pattern repeats every two. */
  --square: clamp(2.25rem, 4.2vw, 4rem);
  --period: calc(var(--square) * 2);

  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;

  /* The board fades out towards the sides and the top, and falls away gently below. */
  mask-image: radial-gradient(ellipse 75% 70% at 50% 46%, #000 10%, transparent 100%);
}

.chequer-board {
  position: absolute;
  inset-block: calc(var(--period) * -1) 0;
  inset-inline: calc(var(--period) * -1) 0;

  /* The dark squares of each period; the light ones are the page itself. */
  background: repeating-conic-gradient(color-mix(in oklab, var(--e-ink) 7%, transparent) 0 25%, transparent 0 50%) 0 0 /
    var(--period) var(--period);

  /* Slow: one period in 48 seconds. */
  animation: drift 48s linear infinite;
  will-change: transform;
}

/* A faint warm glow, fixed to the window, behind the title. */
.chequer::before {
  content: "";
  position: absolute;
  inset: 0;
  background: radial-gradient(
    ellipse 55% 40% at 50% 42%,
    color-mix(in oklab, var(--e-brand) 9%, transparent),
    transparent
  );
}

.paused .chequer-board {
  animation-play-state: paused;
}

@keyframes drift {
  to {
    transform: translate3d(var(--period), var(--period), 0);
  }
}

@media (prefers-reduced-motion: reduce) {
  .chequer-board {
    animation: none;
    transform: translate3d(calc(var(--square) * 0.5), calc(var(--square) * 0.5), 0);
  }
}
</style>
