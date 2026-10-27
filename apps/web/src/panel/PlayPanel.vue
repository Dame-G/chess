<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { EActions } from "@eos-ui/vue/components/EActions";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import type { SeatAction, Side } from "../api/types";
import { other, seatName, speedName, timeControl } from "../format";
import { useBoard } from "../board/board";
import { onLichessAsYou } from "../board/modes";
import { useGames } from "../stores/games";
import { useVersions } from "../stores/versions";

/**
 * Play: You are a seat in a live game. Whose move it is, offers to answer, the opponent leaving and the claims it
 * opens, and the actions the game allows You now. On Lichess nothing else is on screen.
 */
const board = useBoard();
const games = useGames();
const versions = useVersions();
const toast = useToast();

const g = computed(() => board.game);
const s = computed(() => board.summary);
const yourSide = computed<Side>(() => s.value?.you ?? "white");
const theirs = computed(() => other(yourSide.value));
const opponent = computed(() => {
  const seat = s.value?.[theirs.value];
  return seat ? seatName(seat, versions.name(seat.version)) : "Your opponent";
});
const yourTurn = computed(() => board.atEnd && board.position.turn === yourSide.value);
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

const status = computed(() => {
  if (!s.value) return "";
  if (s.value.status === "starting") return "The game is starting.";
  if (yourTurn.value) return "Your move.";
  const seat = s.value[theirs.value];
  return seat.player === "dameless" ? `${opponent.value} is thinking.` : `Waiting for ${opponent.value}.`;
});
const where = computed(() => {
  const x = s.value;
  if (!x) return "";
  const tc = timeControl(x.time);
  if (x.source.type === "lichess")
    return `${tc} · ${speedName[x.source.speed]} on Lichess${x.source.rated ? ", rated" : ""}`;
  return `${tc} · here`;
});
</script>

<template>
  <div class="play">
    <p class="status" aria-live="polite">{{ status }}</p>
    <p class="where">{{ where }}</p>

    <ENotice v-if="g?.offers.draw === theirs" tone="info" :title="`${opponent} offers a draw`">
      <EActions>
        <EButton size="sm" variant="outline" :loading="busy === 'accept_draw'" @click="act('accept_draw')">
          Accept
        </EButton>
        <EButton size="sm" variant="outline" :loading="busy === 'decline_draw'" @click="act('decline_draw')">
          Decline
        </EButton>
      </EActions>
    </ENotice>
    <ENotice v-if="g?.offers.takeback === theirs" tone="info" :title="`${opponent} asks to take back a move`">
      <EActions>
        <EButton size="sm" variant="outline" @click="act('accept_takeback')">Accept</EButton>
        <EButton size="sm" variant="outline" @click="act('decline_takeback')">Decline</EButton>
      </EActions>
    </ENotice>
    <p v-if="g?.offers.draw === yourSide" class="note">You offered a draw.</p>
    <p v-if="g?.offers.takeback === yourSide" class="note">You asked to take back a move.</p>

    <ENotice v-if="games.opponentGone" tone="warning" :title="`${opponent} left the game`">
      <template v-if="claimIn > 0">
        You can claim the game in <span class="num">{{ claimIn }}</span> s.
      </template>
      <EActions v-else>
        <EButton v-if="can('claim_victory')" size="sm" variant="outline" @click="act('claim_victory')">
          Claim victory
        </EButton>
        <EButton v-if="can('claim_draw')" size="sm" variant="outline" @click="act('claim_draw')">
          Call it a draw
        </EButton>
      </EActions>
    </ENotice>

    <EActions class="actions">
      <EButton
        v-if="can('offer_draw')"
        size="sm"
        variant="outline"
        :loading="busy === 'offer_draw'"
        @click="act('offer_draw')"
      >
        Offer a draw
      </EButton>
      <EButton
        v-if="can('takeback')"
        size="sm"
        variant="outline"
        :loading="busy === 'takeback'"
        @click="act('takeback')"
      >
        Take back
      </EButton>
      <EButton v-if="can('abort')" size="sm" variant="outline" @click="act('abort')">Abort</EButton>
      <EButton v-if="can('resign')" size="sm" variant="danger" @click="confirmResign = true">Resign</EButton>
    </EActions>

    <p v-if="s && onLichessAsYou(s)" class="note">Analysis is off while you're playing on Lichess.</p>

    <EDialog v-model:open="confirmResign" title="Resign this game?" role="alertdialog" size="sm">
      {{ opponent }} wins the game.
      <template #actions>
        <EButton variant="danger" @click="resign">Resign</EButton>
        <EButton variant="outline" @click="confirmResign = false">Keep playing</EButton>
      </template>
    </EDialog>
  </div>
</template>

<style scoped>
.play {
  display: grid;
  gap: var(--e-gap-snug);
}

.status {
  margin: 0;
  font-weight: 600;
}

.where,
.note {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.actions {
  margin-block-start: var(--e-gap-tight);
}
</style>
