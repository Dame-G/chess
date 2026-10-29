<script setup lang="ts">
import { computed } from "vue";
import type { GameSummary } from "../api/types";
import type { Mode } from "../board/modes";
import { endingText, resultOf, timeControl } from "../format";
import UiKeyValue, { type KvItem } from "./UiKeyValue.vue";
import UiPlayer from "./UiPlayer.vue";
import UiResult from "./UiResult.vue";

/**
 * The game card at the top of the board panel, and the one place the panel says who is playing whom: the two players
 * with the result between them, then one muted line (`Opening · 5+3 · rated · Lichess`). `#status` is what is
 * happening now ("Your move", "Dameless is thinking"); `#actions` sit beside it. A finished game with no `#status`
 * says how it ended. With no game (free analysis) it draws nothing.
 */
const props = defineProps<{ game: GameSummary | null | undefined; mode: Mode }>();
defineSlots<{ status?(): unknown; actions?(): unknown }>();

const SOURCE = { local: "local", lichess: "Lichess", import: "imported", trial: "trial" } as const;

const facts = computed<KvItem[]>(() => {
  const g = props.game;
  if (!g) return [];
  const items: KvItem[] = [];
  if (g.opening?.name) items.push({ key: "opening", label: "Opening", value: g.opening.name });
  items.push({ key: "clock", label: "Time control", value: timeControl(g.time) });
  const s = g.source;
  if (s.type === "lichess" || s.type === "import") {
    items.push({ key: "rated", label: "Rating", value: s.rated ? "rated" : "casual" });
  }
  items.push({ key: "source", label: "Where", value: SOURCE[s.type] });
  return items;
});

const ended = computed(() =>
  props.game && props.game.status === "finished"
    ? endingText(props.game.ending ?? "aborted", props.game.outcome)
    : null,
);
</script>

<template>
  <header v-if="game" class="ui-head" :data-mode="mode">
    <div class="ui-head-players">
      <UiPlayer :seat="game.white" colour="white" class="ui-head-side" />
      <UiResult
        :result="resultOf(game.outcome) ?? (game.status === 'live' ? '*' : null)"
        :you="game.you"
        class="ui-head-result"
      />
      <UiPlayer :seat="game.black" colour="black" class="ui-head-side ui-head-side--black" />
    </div>
    <UiKeyValue inline :items="facts" />
    <div v-if="$slots['status'] || $slots['actions'] || ended" class="ui-head-foot">
      <p class="ui-head-status">
        <slot name="status">{{ ended }}</slot>
      </p>
      <div v-if="$slots['actions']" class="ui-head-actions"><slot name="actions" /></div>
    </div>
  </header>
</template>

<style scoped>
.ui-head {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
  padding: var(--sp-3);
}

.ui-head-players {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: var(--sp-2);
}

.ui-head-side--black {
  justify-self: end;
}

.ui-head-result {
  min-width: 3rem;
  text-align: center;
}

.ui-head-foot {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-2);
  margin-block-start: var(--sp-1);
}

.ui-head-status {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  margin: 0;
  color: var(--c-ink);
  font: var(--t-body);
}

.ui-head-actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-2);
  margin-inline-start: auto;
}
</style>
