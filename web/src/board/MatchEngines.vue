<script setup lang="ts">
import type { Side } from "./matchText";

export interface EngineRow {
  side: Side;
  /** The build's ref. */
  name: string;
  /** The evaluation from White's side, such as `+0.35`, or `null` before the engine has spoken. */
  eval: string | null;
  depth: string;
  speed: string;
  /** The line it is thinking along, or the move it played, in SAN. */
  line: string;
  thinking: boolean;
}

defineProps<{ rows: EngineRow[] }>();
</script>

<template>
  <div class="engines">
    <section
      v-for="r in rows"
      :key="r.side"
      :class="['eng', `eng--${r.side}`, { 'eng--on': r.thinking }]"
      :aria-label="`${r.side === 'white' ? 'White' : 'Black'} engine`"
    >
      <header class="head">
        <span class="swatch" aria-hidden="true"></span>
        <span class="side">{{ r.side === "white" ? "White" : "Black" }}</span>
        <span class="name">{{ r.name }}</span>
        <span v-if="r.thinking" class="state">Thinking</span>
      </header>
      <div class="body">
        <div class="eval num" :class="{ dim: r.eval === null }">{{ r.eval ?? "–" }}</div>
        <div class="facts num">
          <span>{{ r.depth }}</span>
          <span>{{ r.speed }}</span>
        </div>
      </div>
      <p class="line num" :class="{ dim: !r.thinking }">{{ r.line }}</p>
    </section>
  </div>
</template>

<style scoped>
.engines {
  display: grid;
}

.eng {
  display: grid;
  gap: var(--e-gap-tight);
  padding: var(--e-gap-snug) var(--ws-pad);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
}

/* The engine on the move: the cognac edge the player strips use. */
.eng--on {
  box-shadow: inset 3px 0 0 var(--e-primary);
}

.eng:last-child {
  border-block-end: 0;
}

.head {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  min-width: 0;
  font: var(--e-type-small);
}

/* The colour it plays, as a piece: the same chips as the players' strips. */
.swatch {
  flex: none;
  width: 0.75rem;
  height: 0.75rem;
  border-radius: 50%;
  box-shadow: 0 0 0 1px var(--e-border-strong);
}

.eng--white .swatch {
  background: #fffaf3;
}

.eng--black .swatch {
  background: #2a2420;
}

.side {
  font-weight: 600;
}

.name {
  overflow: hidden;
  color: var(--e-muted);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.state {
  margin-inline-start: auto;
  color: var(--e-primary);
  font-weight: 600;
}

.body {
  display: flex;
  align-items: baseline;
  gap: var(--e-gap-loose);
}

.eval {
  min-width: 5.2ch;
  font: 600 var(--e-text-2xl) / 1.1 var(--e-font-mono);
  letter-spacing: -0.03em;
}

.facts {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-base);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.dim {
  color: var(--e-muted);
}

/* Two rows of moves, always: a longer or shorter line never changes the height of what is below. */
.line {
  height: 2.6rem;
  margin: 0;
  overflow: hidden;
  font: var(--e-type-small);
  line-height: 1.3rem;
}
</style>
