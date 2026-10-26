<script setup lang="ts">
import { computed } from "vue";
import { PhArrowBendDownRight, PhCrosshair, PhSkipBack, PhSkipForward } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import ReviewGlyph from "./ReviewGlyph.vue";
import { hasBetter, hint, markOf, scoreText, type PlyReview } from "./review";

/**
 * What the review says about the move on the board: its judgement and score swing in a line, and for a move that could
 * be better the engine's line, to show on the board or play out from the position before. The two arrows walk the
 * moves worth a look.
 */
const props = defineProps<{
  /** The move on the board; none at the start of the game. */
  ply?: PlyReview;
  /** The move as the counter writes it, `24… Nf3`. */
  label: string;
  /** The engine's line from the position before the move, written out. */
  best: string;
  /** The board is on the position before this move, with the best move drawn. */
  shown: boolean;
  /** Whether there is any move to walk to. */
  walk: boolean;
}>();
const emit = defineEmits<{ show: []; play: []; next: [back: boolean] }>();

const mark = computed(() => (props.ply ? markOf(props.ply.classification) : undefined));
const swing = computed(() =>
  props.ply
    ? `${scoreText(props.ply.eval_before, props.ply.side)} → ${scoreText(props.ply.eval_after, props.ply.side)}`
    : "",
);
const better = computed(() => !!props.ply && hasBetter(props.ply));
</script>

<template>
  <section class="rm" aria-label="This move" aria-live="polite">
    <div class="top">
      <p v-if="ply" :class="['mvl', mark?.tone && `rv-${mark.tone}`]">
        <strong class="num">{{ label }}<ReviewGlyph :of="ply.classification" /></strong>
        <span class="swing num">{{ swing }}</span>
      </p>
      <p v-else class="mvl"><span class="quiet">Start of the game</span></p>
      <span class="walk">
        <EIconButton
          :icon="PhSkipBack"
          label="Previous mistake (Shift X)"
          tooltip
          size="sm"
          :disabled="!walk"
          @click="emit('next', true)"
        />
        <EIconButton
          :icon="PhSkipForward"
          label="Next mistake (X)"
          tooltip
          size="sm"
          :disabled="!walk"
          @click="emit('next', false)"
        />
      </span>
    </div>
    <p v-if="ply" class="say">{{ hint(ply) }}</p>
    <template v-if="ply && better">
      <p class="line num"><span class="quiet">Best</span> {{ best }}</p>
      <div class="act">
        <EButton variant="outline" size="sm" :icon="PhCrosshair" :aria-pressed="shown" @click="emit('show')">
          {{ shown ? "Hide best move" : "Show best move" }}
        </EButton>
        <EButton variant="outline" size="sm" :icon="PhArrowBendDownRight" @click="emit('play')">
          Play best line
        </EButton>
      </div>
    </template>
  </section>
</template>

<style scoped>
.rm {
  display: grid;
  gap: var(--e-gap-tight);
  padding: var(--e-gap-snug) var(--ws-pad);
}

.rm p {
  margin: 0;
}

.top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-snug);
}

.mvl {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0 var(--e-gap-base);
  min-width: 0;
}

.mvl strong {
  font: 600 var(--e-text-base) / 1.3 var(--e-font-mono);
}

.swing,
.quiet {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.walk {
  display: flex;
  flex: none;
  gap: var(--e-gap-tight);
}

.say {
  font: var(--e-type-small);
}

.line {
  font: var(--e-type-small);
  overflow-wrap: anywhere;
}

.act {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-snug);
}
</style>
