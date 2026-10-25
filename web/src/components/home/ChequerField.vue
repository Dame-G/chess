<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";

// A chequerboard receding to a horizon and sliding slowly towards the viewer. Painted on one canvas, in the theme's ink.

const root = ref<HTMLElement | null>(null);
const canvas = ref<HTMLCanvasElement | null>(null);
/** Rows per second the board travels. Slow enough to read as drift. */
const SPEED = 0.32;
/** The shortest wait between redraws, ms. */
const MIN_GAP = 15;
/** The farthest row, in squares. */
const FAR = 44;
/** The nearest row, in squares: rows past it leave through the bottom edge. */
const NEAR = 0.3;
/** Where the horizon sits, as a share of the height. */
const HORIZON = 0.3;
/** Ink opacity of the nearest dark squares. */
const PEAK = 0.07;
/** The still frame for reduced motion, in seconds. */
const STILL = 7;

let cleanup: Array<() => void> = [];

onMounted(() => {
  const el = root.value;
  const cv = canvas.value;
  const ctx = cv?.getContext("2d");
  if (!el || !cv || !ctx) return;

  let ink = "";
  let brand = "";
  const readColours = () => {
    const style = getComputedStyle(cv);
    ink = style.color;
    brand = style.accentColor;
  };
  readColours();
  let scale = Math.min(2, devicePixelRatio || 1);
  let w = 0;
  let h = 0;

  const paint = (seconds: number) => {
    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const hy = h * HORIZON;
    const depth = (h - hy) * 0.9;
    const reach = depth * 1.35;
    const t = seconds * SPEED;
    const sway = seconds * 0.05;
    const cx = w / 2;

    ctx.fillStyle = ink;
    // Far rows first, so a nearer row is never under a farther one. Each row is one path, so squares that touch never double up.
    for (let n = Math.ceil(t + NEAR) + 1; n - t < FAR; n++) {
      const z0 = n - 1 - t < NEAR ? NEAR : n - 1 - t;
      const z1 = n - t;
      if (z1 <= NEAR) continue;
      const y0 = hy + depth / z0;
      const y1 = hy + depth / z1;
      if (y0 < 0 || y1 > h + 4) continue;
      const near = reach / z0;
      const far = reach / z1;
      const half = Math.ceil(cx / Math.min(near, far)) + 1;
      const first = Math.floor(sway) - half;
      ctx.globalAlpha = PEAK * Math.min(1, (1 - z1 / FAR) ** 1.6 * 1.6);
      ctx.beginPath();
      for (let i = first; i <= Math.floor(sway) + half; i++) {
        if (((i + n) & 1) === 0) continue;
        const l = i - sway;
        ctx.moveTo(cx + l * near, y0);
        ctx.lineTo(cx + (l + 1) * near, y0);
        ctx.lineTo(cx + (l + 1) * far, y1);
        ctx.lineTo(cx + l * far, y1);
        ctx.closePath();
      }
      ctx.fill();
    }

    // A hairline where the board meets the sky, in the brand colour.
    ctx.globalAlpha = 0.35;
    const line = ctx.createLinearGradient(0, 0, w, 0);
    line.addColorStop(0, "transparent");
    line.addColorStop(0.5, brand);
    line.addColorStop(1, "transparent");
    ctx.fillStyle = line;
    ctx.fillRect(0, Math.round(hy) - 0.5, w, 1);
    ctx.globalAlpha = 1;
  };

  const clock = () => performance.now() / 1000;

  const fit = () => {
    scale = Math.min(2, devicePixelRatio || 1);
    w = el.clientWidth;
    h = el.clientHeight;
    cv.width = Math.round(w * scale);
    cv.height = Math.round(h * scale);
  };
  fit();

  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const draw = () => paint(reduced ? STILL : clock());
  draw();

  const resize = new ResizeObserver(() => {
    fit();
    draw();
  });
  resize.observe(el);
  cleanup.push(() => resize.disconnect());

  // The ink changes with the colour scheme, whether the page follows the system or sets it itself.
  const rescheme = () => {
    readColours();
    draw();
  };
  const observer = new MutationObserver(rescheme);
  observer.observe(document.documentElement, { attributes: true });
  const dark = matchMedia("(prefers-color-scheme: dark)");
  dark.addEventListener("change", rescheme);
  cleanup.push(() => {
    observer.disconnect();
    dark.removeEventListener("change", rescheme);
  });

  if (reduced) return;

  // Redraw no faster than a redraw is cheap: the wait is four times the last one's cost. Nothing is drawn while the tab is hidden.
  let last = 0;
  let gap = MIN_GAP;
  let alive = true;
  const loop = (now: number) => {
    if (!alive) return;
    requestAnimationFrame(loop);
    if (now - last < gap || document.visibilityState === "hidden") return;
    last = now;
    const start = performance.now();
    paint(clock());
    gap = Math.min(500, Math.max(MIN_GAP, (performance.now() - start) * 4));
  };
  requestAnimationFrame(loop);
  cleanup.push(() => {
    alive = false;
  });
});

onBeforeUnmount(() => {
  cleanup.forEach((fn) => fn());
  cleanup = [];
});
</script>

<template>
  <div ref="root" class="chequer" aria-hidden="true">
    <canvas ref="canvas" class="chequer-canvas" />
  </div>
</template>

<style scoped>
.chequer {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
}

.chequer-canvas {
  display: block;
  inline-size: 100%;
  block-size: 100%;
  color: var(--e-ink);
  accent-color: var(--e-brand);
}
</style>
