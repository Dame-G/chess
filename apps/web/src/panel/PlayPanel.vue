<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import type { SeatAction, Side } from "../api/types";
import { engineOf, other, seatName } from "../format";
import { useBoard } from "../board/board";
import { useGames } from "../stores/games";
import UiDialog from "../ui/UiDialog.vue";
import UiLive from "../ui/UiLive.vue";
import UiToolbar from "../ui/UiToolbar.vue";
import PanelMenu from "./PanelMenu.vue";

/**
 * Play: You are a seat in a live game. Whose move it is, offers to answer, the opponent leaving and the claims it
 * opens, and the actions the game allows You now. Who is playing whom and the clock are in the game card above.
 */
const board = useBoard();
const games = useGames();
const toast = useToast();

const g = computed(() => board.game);
const s = computed(() => board.summary);
const yourSide = computed<Side>(() => s.value?.you ?? "white");
const theirs = computed(() => other(yourSide.value));
const opponentSeat = computed(() => s.value?.[theirs.value]);
const opponent = computed(() => (opponentSeat.value ? seatName(opponentSeat.value) : "Your opponent"));
/** A person on Lichess can say no to a takeback; an engine, or a bot, is simply asked. */
const person = computed(() => {
  const seat = opponentSeat.value;
  return s.value?.source.type === "lichess" && !!seat?.player.startsWith("lichess:") && seat.title !== "BOT";
});
/** Whose turn it is in the game, wherever the board has been stepped to. */
const turn = computed(() => board.plies[board.lastPly]?.turn);
const can = (a: SeatAction) => !!g.value?.actions.includes(a);

const busy = ref<SeatAction | null>(null);
async function act(a: SeatAction) {
  busy.value = a;
  try {
    const after = await games.act(a);
    // An engine answers a draw offer at once: an offer that is not pending after it was declined.
    if (a === "offer_draw" && after?.summary.status === "live" && !after.offers.draw)
      toast.show({ title: `${opponent.value} declined the draw`, tone: "info" });
  } catch (e) {
    toast.show({
      title: "That did not go through",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  } finally {
    busy.value = null;
  }
}

const confirmResign = ref(false);
async function resign() {
  confirmResign.value = false;
  await act(can("abort") ? "abort" : "resign");
}

/** The opponent left: the seconds until a claim is allowed, counted down here. */
const claimIn = ref(0);
let timer: ReturnType<typeof setInterval> | undefined;
watch(
  () => games.opponentGone,
  (gone) => {
    clearInterval(timer);
    claimIn.value = gone?.claim_in_seconds ?? 0;
    if (gone) timer = setInterval(() => (claimIn.value = Math.max(0, claimIn.value - 1)), 1000);
  },
  { immediate: true },
);
onBeforeUnmount(() => clearInterval(timer));

/** Whose move it is, said as a state: the board shows the last move, so this is only the turn. */
const status = computed(() => {
  if (!s.value) return "";
  if (s.value.status === "starting") return "Starting the game";
  if (turn.value === yourSide.value) return "Your move";
  return engineOf(opponentSeat.value?.player ?? "") ? `${opponent.value} is thinking` : `Waiting for ${opponent.value}`;
});
</script>

<template>
  <div class="play">
    <p class="turn" :class="{ 'turn--yours': turn === yourSide }" aria-live="polite">
      <UiLive :pulse="board.lastPly" />
      {{ status }}
    </p>

    <div v-if="g?.offers.draw === theirs" class="offer" role="group" :aria-label="`${opponent} offers a draw`">
      <p class="offer-text">{{ opponent }} offers a draw</p>
      <UiToolbar>
        <EButton variant="outline" :loading="busy === 'accept_draw'" @click="act('accept_draw')">Accept</EButton>
        <EButton variant="quiet" :loading="busy === 'decline_draw'" @click="act('decline_draw')">Decline</EButton>
      </UiToolbar>
    </div>
    <div
      v-if="g?.offers.takeback === theirs"
      class="offer"
      role="group"
      :aria-label="`${opponent} asks to take back a move`"
    >
      <p class="offer-text">{{ opponent }} asks to take back a move</p>
      <UiToolbar>
        <EButton variant="outline" @click="act('accept_takeback')">Accept</EButton>
        <EButton variant="quiet" @click="act('decline_takeback')">Decline</EButton>
      </UiToolbar>
    </div>
    <p v-if="g?.offers.draw === yourSide" class="note">You offered a draw.</p>
    <p v-if="g?.offers.takeback === yourSide" class="note">You asked to take back a move.</p>

    <div v-if="games.opponentGone" class="offer offer--warn" role="group" :aria-label="`${opponent} left the game`">
      <p class="offer-text">{{ opponent }} left the game</p>
      <p v-if="claimIn > 0" class="note">
        You can claim the game in <span class="num">{{ claimIn }}</span> s.
      </p>
      <UiToolbar v-else>
        <EButton v-if="can('claim_victory')" variant="outline" @click="act('claim_victory')">Claim victory</EButton>
        <EButton v-if="can('claim_draw')" variant="outline" @click="act('claim_draw')">Call it a draw</EButton>
      </UiToolbar>
    </div>

    <UiToolbar align="between" label="Game actions">
      <EButton v-if="can('offer_draw')" variant="outline" :loading="busy === 'offer_draw'" @click="act('offer_draw')">
        Offer draw
      </EButton>
      <EButton
        v-if="can('takeback')"
        variant="outline"
        :loading="busy === 'takeback'"
        :title="person ? undefined : 'Take back your last move and the reply (T)'"
        :aria-keyshortcuts="person ? undefined : 'T'"
        @click="act('takeback')"
      >
        {{ person ? "Ask for takeback" : "Take back" }}
      </EButton>
      <EButton v-if="can('abort')" variant="outline" @click="act('abort')">Abort</EButton>
      <template #end>
        <EButton v-if="can('resign')" variant="quiet" class="resign" @click="confirmResign = true">Resign</EButton>
        <PanelMenu />
      </template>
    </UiToolbar>

    <UiDialog
      v-model:open="confirmResign"
      title="Resign this game?"
      size="sm"
      alert
      danger
      confirm-label="Resign"
      @confirm="resign"
    >
      You lose the game.
    </UiDialog>
  </div>
</template>

<style scoped>
.play {
  display: grid;
  gap: var(--sp-3);
}

.turn {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-body-strong);
}

.turn--yours {
  color: var(--c-ink);
}

.note {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

/* A question put to You: marked on the start edge, in the quiet raised tone, so it is seen without shouting. */
.offer {
  display: grid;
  gap: var(--sp-2);
  padding: var(--sp-2) var(--sp-3);
  border-radius: var(--r-1);
  background: var(--c-raise);
  box-shadow: inset var(--e-stroke-focus) 0 0 var(--c-act);
}

.offer--warn {
  box-shadow: inset var(--e-stroke-focus) 0 0 var(--c-warn);
}

.offer-text {
  margin: 0;
  font: var(--t-body-strong);
}

.resign {
  color: var(--c-loss);
}
</style>
