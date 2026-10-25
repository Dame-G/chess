<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";

// A chequerboard receding to a horizon and drifting towards the viewer, painted on one canvas in the theme's ink.
// Rows are perspective-correct and fade with distance and with thinness (so they never shimmer or pop), every square
// stays drawn until it has left the canvas, the vanishing point eases towards the pointer, and a knight wanders across.

const root = ref<HTMLElement | null>(null);
const canvas = ref<HTMLCanvasElement | null>(null);
const glow = ref<HTMLElement | null>(null);
/** Rows per second the board travels: about one square every three seconds. */
const SPEED = 0.3;
/** Sideways drift, in squares per second. */
const DRIFT = 0.03;
/** The shortest wait between redraws, ms. */
const MIN_GAP = 15;
/** The farthest row, in squares. */
const FAR = 36;
/** Nearest depth a square is drawn to; far below the canvas, so only the clip matters. */
const Z_MIN = 0.05;
/** Where the horizon sits, as a share of the height. */
const HORIZON = 0.34;
/** Ink opacity of the nearest dark squares. */
const PEAK = 0.1;
/** Peak opacity of the knight's squares. */
const TRAIL = 0.42;
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

  // Colours are read from the computed custom properties (via `color` and `accent-color` on the canvas), so they follow
  // whatever sets the theme: the system scheme, or an attribute or class on <html>.
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
    // Squares fade out on their own; they are dropped only once nothing of them is left.
    while (trail.length && seconds - trail[0]!.at > HOP * 2.8) trail.shift();
    const step = Math.floor(seconds / HOP);
    if (step === hops) return;
    hops = step;
    const last = trail[trail.length - 1];
    // Restart when the trail has left the board or is empty; the knight lands a few rows out, off to a side.
    if (!last || last.r - t < 3) {
      const c = Math.round(Math.sin(step * 2.4) * 3);
      trail.push({ c, r: Math.ceil(t) + 14, at: seconds });
      return;
    }
    // Prefer a hop towards the viewer and back to the middle, so the walk stays in view.
    const options = KNIGHT.filter(([dc, dr]) => dr < 0 && Math.abs(last.c + dc) <= 5);
    const [dc, dr] = options[step % options.length] ?? [1, -2];
    trail.push({ c: last.c + dc, r: last.r + dr, at: seconds });
  };

  /** Adds one square of the board to the current path. Both edges lie on one ray through the vanishing point, so a
   * near edge clamped far below the canvas is exact for everything that is visible. */
  const square = (cx: number, hy: number, depth: number, reach: number, l: number, za: number, zb: number) => {
    const z0 = Math.max(Z_MIN, za);
    const near = reach / z0;
    const far = reach / zb;
    const y0 = hy + depth / z0;
    const y1 = hy + depth / zb;
    ctx.moveTo(cx + l * near, y0);
    ctx.lineTo(cx + (l + 1) * near, y0);
    ctx.lineTo(cx + (l + 1) * far, y1);
    ctx.lineTo(cx + l * far, y1);
    ctx.closePath();
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
    // Squares narrow on tall, narrow screens, so a phone still sees a board and not a few giant tiles.
    const reach = depth * 1.35 * Math.min(1, Math.max(0.5, (w / h) * 1.6));
    const t = seconds * SPEED;
    const sway = seconds * DRIFT;
    const cx = w / 2 + viewX * w * 0.04;
    hop(seconds, t);
    if (glow.value) {
      glow.value.style.transform = `translate3d(${(cx - w / 2).toFixed(2)}px, ${(hy - h * HORIZON).toFixed(2)}px, 0)`;
    }

    ctx.fillStyle = ink;
    // Far rows first. Each row is one path, so squares that touch never double up. A row stays until its far edge has
    // left the bottom of the canvas, and a row's squares stay until they have left the sides.
    for (let n = Math.floor(t) + 1; n - t < FAR; n++) {
      const za = n - 1 - t;
      const z1 = n - t;
      const y1 = hy + depth / z1;
      if (y1 >= h + 1) continue;
      const y0 = hy + depth / Math.max(Z_MIN, za);
      const thin = smooth(THIN[0], THIN[1], y0 - y1);
      if (thin <= 0) continue;
      // The narrowest a square gets on this row is at its far edge, so that edge decides which columns reach the canvas.
      const far = reach / z1;
      const first = Math.floor(sway - cx / far) - 2;
      const last = Math.ceil(sway + (w - cx) / far) + 2;
      const fog = (1 - z1 / FAR) ** 1.3;
      ctx.globalAlpha = PEAK * fog * thin * (1 + 0.5 * smooth(6, 1, z1));
      ctx.beginPath();
      for (let i = first; i <= last; i++) {
        if (((i + n) & 1) === 0) continue;
        square(cx, hy, depth, reach, i - sway, za, z1);
      }
      ctx.fill();
    }

    // The knight's trail: the newest square glows in the brand colour, older ones fade away.
    ctx.fillStyle = brand;
    for (const p of trail) {
      const age = seconds - p.at;
      const life = smooth(0, 1.2, age) * (1 - smooth(HOP * 1.2, HOP * 2.8, age));
      const za = p.r - 1 - t;
      const z1 = p.r - t;
      if (life <= 0 || z1 <= 0 || z1 >= FAR) continue;
      const y1 = hy + depth / z1;
      if (y1 >= h + 1) continue;
      const thin = smooth(THIN[0], THIN[1], hy + depth / Math.max(Z_MIN, za) - y1);
      ctx.globalAlpha = TRAIL * life * thin * (1 - z1 / FAR) ** 0.6;
      ctx.beginPath();
      square(cx, hy, depth, reach, p.c - sway, za, z1);
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

  // The ink changes with the colour scheme, whether the page follows the system or sets it itself (an attribute or class
  // on <html>). A switch may transition, so the colours are read again a moment later.
  let settle = 0;
  const rescheme = () => {
    readColours();
    draw();
    clearTimeout(settle);
    settle = window.setTimeout(() => {
      readColours();
      draw();
    }, 400);
  };
  const observer = new MutationObserver(rescheme);
  observer.observe(document.documentElement, { attributes: true });
  observer.observe(document.body, { attributes: true });
  cleanup.push(() => {
    observer.disconnect();
    clearTimeout(settle);
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
  let lastRead = 0;
  let gap = MIN_GAP;
  let alive = true;
  const loop = (now: number) => {
    if (!alive) return;
    requestAnimationFrame(loop);
    if (now - last < gap || !visible || document.visibilityState === "hidden") return;
    last = now;
    if (now - lastRead > 1000) {
      lastRead = now;
      readColours();
    }
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
    <div ref="glow" class="chequer-glow" />
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

/* A warm glow where the board meets the sky, behind the title. Extra stops keep the long fall-off smooth. */
.chequer-glow {
  position: absolute;
  inset: -30px -80px;
  background:
    radial-gradient(
      ellipse 55% 26% at 50% 34%,
      color-mix(in oklab, var(--e-brand) 20%, transparent),
      color-mix(in oklab, var(--e-brand) 14%, transparent) 20%,
      color-mix(in oklab, var(--e-brand) 8%, transparent) 40%,
      color-mix(in oklab, var(--e-brand) 3%, transparent) 60%,
      transparent 80%
    ),
    radial-gradient(
      ellipse 90% 10% at 50% 34%,
      color-mix(in oklab, var(--e-brand) 12%, transparent),
      color-mix(in oklab, var(--e-brand) 6%, transparent) 40%,
      color-mix(in oklab, var(--e-brand) 2%, transparent) 70%,
      transparent 100%
    );
  will-change: transform;
}

/* Fine grain over the glow, so its soft gradient does not band. */
.chequer-glow::after {
  content: "";
  position: absolute;
  inset: 0;
  opacity: 0.05;
  mix-blend-mode: overlay;
  background-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='128' height='128'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/><feColorMatrix values='0 0 0 0 .5 0 0 0 0 .5 0 0 0 0 .5 1 0 0 0 0'/></filter><rect width='128' height='128' filter='url(%23n)'/></svg>");
}

/* No edge mask: every square stays until it has left the canvas. Distance fog is painted into the squares. */
.chequer-canvas {
  display: block;
  inline-size: 100%;
  block-size: 100%;
  color: var(--e-ink);
  accent-color: var(--e-brand);
}
</style>
