<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";

// A chequerboard receding to a horizon and drifting towards the viewer, painted on one canvas in the theme's ink.
// Rows are perspective-correct, fade with distance and with thinness (so they never shimmer), the vanishing point
// eases towards the pointer, and a knight wanders across the squares.

const root = ref<HTMLElement | null>(null);
const canvas = ref<HTMLCanvasElement | null>(null);
/** Rows per second the board travels: about one square every three seconds. */
const SPEED = 0.3;
/** Sideways drift, in squares per second. */
const DRIFT = 0.03;
/** The shortest wait between redraws, ms. */
const MIN_GAP = 15;
/** The farthest row, in squares. */
const FAR = 36;
/** The nearest row, in squares: rows past it leave through the bottom edge. */
const NEAR = 0.3;
/** Where the horizon sits, as a share of the height. */
const HORIZON = 0.34;
/** Ink opacity of the nearest dark squares. */
const PEAK = 0.1;
/** Rows thinner than the first number (px) vanish; fully drawn from the second. */
const THIN: [number, number] = [1.2, 3];
/** The still frame for reduced motion, in seconds. */
const STILL = 7;
/** Seconds between the knight's hops. */
const HOP = 4;
const KNIGHT: Array<[number, number]> = [
  [2, 1],
  [1, 2],
  [-1, 2],
  [-2, 1],
  [-2, -1],
  [-1, -2],
  [1, -2],
  [2, -1],
];

let cleanup: Array<() => void> = [];

const smooth = (a: number, b: number, x: number) => {
  const t = Math.min(1, Math.max(0, (x - a) / (b - a)));
  return t * t * (3 - 2 * t);
};

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

  // The pointer, as -1..1 from the centre; the view eases towards it.
  let aimX = 0;
  let aimY = 0;
  let viewX = 0;
  let viewY = 0;
  let lastSeconds = 0;

  // The knight's trail, in board squares (column, row), newest last.
  const trail: Array<{ c: number; r: number; at: number }> = [];
  let hops = -1;
  const hop = (seconds: number, t: number) => {
    const step = Math.floor(seconds / HOP);
    if (step === hops) return;
    hops = step;
    const last = trail[trail.length - 1];
    // Restart when the trail has left the board or is empty; the knight lands a few rows out, off to a side.
    if (!last || last.r - t < 3) {
      const c = Math.round(Math.sin(step * 2.4) * 3);
      trail.length = 0;
      trail.push({ c, r: Math.ceil(t) + 14, at: seconds });
      return;
    }
    // Prefer a hop towards the viewer and back to the middle, so the walk stays in view.
    const options = KNIGHT.filter(([dc, dr]) => dr < 0 && Math.abs(last.c + dc) <= 5);
    const [dc, dr] = options[step % options.length] ?? [1, -2];
    trail.push({ c: last.c + dc, r: last.r + dr, at: seconds });
    if (trail.length > 3) trail.shift();
  };

  const paint = (seconds: number) => {
    const dt = Math.min(0.1, Math.max(0, seconds - lastSeconds));
    lastSeconds = seconds;
    const ease = 1 - Math.exp(-dt * 2.2);
    viewX += (aimX - viewX) * ease;
    viewY += (aimY - viewY) * ease;

    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const hy = h * HORIZON + viewY * h * 0.025;
    const depth = (h - hy) * 0.9;
    const reach = depth * 1.35;
    const t = seconds * SPEED;
    const sway = seconds * DRIFT;
    const cx = w / 2 + viewX * w * 0.04;
    hop(seconds, t);

    ctx.fillStyle = ink;
    // Far rows first, so a nearer row is never under a farther one. Each row is one path, so squares that touch never double up.
    for (let n = Math.ceil(t + NEAR) + 1; n - t < FAR; n++) {
      const z0 = n - 1 - t < NEAR ? NEAR : n - 1 - t;
      const z1 = n - t;
      if (z1 <= NEAR) continue;
      const y0 = hy + depth / z0;
      const y1 = hy + depth / z1;
      if (y0 < 0 || y1 > h + 4) continue;
      const thin = smooth(THIN[0], THIN[1], y0 - y1);
      if (thin <= 0.01) continue;
      const near = reach / z0;
      const far = reach / z1;
      const half = Math.ceil(cx / Math.min(near, far)) + 2;
      const first = Math.floor(sway) - half;
      const fog = (1 - z1 / FAR) ** 1.3;
      ctx.globalAlpha = PEAK * fog * thin * (1 + 0.5 * smooth(6, 1, z1));
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

    // The knight's trail: the newest square glows in the brand colour, older ones fade away.
    ctx.fillStyle = brand;
    for (const p of trail) {
      const age = seconds - p.at;
      const life = smooth(0, 0.8, age) * (1 - smooth(HOP * 1.2, HOP * 2.8, age));
      const z0 = p.r - 1 - t;
      const z1 = p.r - t;
      if (life <= 0 || z0 < NEAR || z1 > FAR) continue;
      const y0 = hy + depth / z0;
      const y1 = hy + depth / z1;
      const thin = smooth(THIN[0], THIN[1], y0 - y1);
      const l = p.c - sway;
      const near = reach / z0;
      const far = reach / z1;
      ctx.globalAlpha = 0.28 * life * thin * (1 - z1 / FAR);
      ctx.beginPath();
      ctx.moveTo(cx + l * near, y0);
      ctx.lineTo(cx + (l + 1) * near, y0);
      ctx.lineTo(cx + (l + 1) * far, y1);
      ctx.lineTo(cx + l * far, y1);
      ctx.closePath();
      ctx.fill();
    }

    // A hairline where the board meets the sky, in the brand colour, snapped to a device pixel.
    ctx.globalAlpha = 0.55;
    const line = ctx.createLinearGradient(0, 0, w, 0);
    line.addColorStop(0, "transparent");
    line.addColorStop(0.5, brand);
    line.addColorStop(1, "transparent");
    ctx.fillStyle = line;
    ctx.fillRect(0, Math.round(hy * scale) / scale - 0.5 / scale, w, 1 / scale);
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

  const point = (e: PointerEvent) => {
    if (e.pointerType === "touch") return;
    aimX = (e.clientX / innerWidth - 0.5) * 2;
    aimY = (e.clientY / innerHeight - 0.5) * 2;
  };
  const leave = () => {
    aimX = 0;
    aimY = 0;
  };
  addEventListener("pointermove", point, { passive: true });
  document.documentElement.addEventListener("pointerleave", leave);
  cleanup.push(() => {
    removeEventListener("pointermove", point);
    document.documentElement.removeEventListener("pointerleave", leave);
  });

  // Redraw no faster than a redraw is cheap: the wait is four times the last one's cost. Nothing is drawn while the tab
  // is hidden or the field is off screen.
  let visible = true;
  const io = new IntersectionObserver((entries) => {
    visible = entries.some((entry) => entry.isIntersecting);
  });
  io.observe(el);
  cleanup.push(() => io.disconnect());

  let last = 0;
  let gap = MIN_GAP;
  let alive = true;
  const loop = (now: number) => {
    if (!alive) return;
    requestAnimationFrame(loop);
    if (now - last < gap || !visible || document.visibilityState === "hidden") return;
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
    <div class="chequer-glow" />
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

/* A warm glow where the board meets the sky, behind the title. */
.chequer-glow {
  position: absolute;
  inset: 0;
  background:
    radial-gradient(ellipse 55% 26% at 50% 34%, color-mix(in oklab, var(--e-brand) 20%, transparent), transparent 70%),
    radial-gradient(ellipse 90% 10% at 50% 34%, color-mix(in oklab, var(--e-brand) 12%, transparent), transparent 80%);
}

/* The board dissolves towards the edges and the horizon's far corners. */
.chequer-canvas {
  display: block;
  inline-size: 100%;
  block-size: 100%;
  color: var(--e-ink);
  accent-color: var(--e-brand);
  mask-image: radial-gradient(ellipse 85% 100% at 50% 34%, #000 30%, transparent 100%);
}
</style>
