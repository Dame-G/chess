<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ETable } from "@eos-ui/vue/components/ETable";
import "../board/review.css";
import type { Classification, GameId, Moment, Profile, WeakSpot } from "../api/types";
import { markOf } from "../board/review";
import { figure, percent, plural, resultText, shortDate } from "../format";
import { readClock } from "../shell/commands";
import { playOutSeat } from "../sheets/levels";
import { useDameless } from "../stores/dameless";
import { useGames } from "../stores/games";
import { stored } from "../stored";
import { speedName } from "../format";
import TrendChart from "./TrendChart.vue";
import Term from "./Term.vue";
import WdlBar from "./WdlBar.vue";

/**
 * A player's games added up, the same building blocks for You, Dameless and anyone on Lichess: key figures, the trend,
 * accuracy by phase and speed, results, moves per game, openings, time, the best moments and worst blunders (links onto
 * the board) and, for You, the weak spots to play out again. What needs a review says so while there is none.
 */
const props = defineProps<{ profile: Profile; isYou?: boolean }>();

const router = useRouter();
const toast = useToast();
const games = useGames();
const dameless = useDameless();
const clock = stored("new.clock", "5+3");

const reviewed = computed(() => props.profile.reviewed > 0);
const phases = computed(() => [
  { label: "Opening", value: props.profile.accuracy.opening },
  { label: "Middlegame", value: props.profile.accuracy.middlegame },
  { label: "Endgame", value: props.profile.accuracy.endgame },
]);
const speeds = computed(() =>
  props.profile.accuracy.by_speed
    .filter((s) => s.speed && s.accuracy != null)
    .map((s) => ({ label: speedName[s.speed!], value: s.accuracy, note: plural(s.reviewed, "game") })),
);

const sides = computed(() => [
  { label: "All games", wdl: props.profile.results.overall },
  { label: "As White", wdl: props.profile.results.white },
  { label: "As Black", wdl: props.profile.results.black },
]);

const CLASS_ROWS: Classification[] = [
  "brilliant",
  "great",
  "best",
  "excellent",
  "good",
  "book",
  "inaccuracy",
  "mistake",
  "blunder",
  "miss",
];
const classRows = computed(() =>
  CLASS_ROWS.map((c) => ({
    c,
    label: markOf(c).label,
    glyph: markOf(c).glyph,
    tone: markOf(c).tone,
    perGame: props.profile.counts_per_game[c],
    total: props.profile.counts[c],
  })),
);

const seconds = (ms: number | null | undefined) => (ms == null ? "—" : `${(ms / 1000).toFixed(1)} s`);

const boardTo = (game: GameId, ply?: number) => ({
  path: "/",
  query: { game: String(game), ...(ply ? { ply: String(ply) } : {}) },
});
const momentText = (m: Moment) => `${Math.ceil(m.ply / 2)}${m.side === "white" ? "." : "…"} ${m.san}`;
const momentNote = (m: Moment) =>
  `${markOf(m.classification).label} against ${m.opponent}${m.sacrifice >= 1 ? `, ${m.sacrifice} pawns given up` : ""}`;
const swing = (m: Moment) => `${m.win_before.toFixed(0)}% → ${m.win_after.toFixed(0)}%`;

const starting = ref<string | null>(null);
/** Starts a local game against Dameless from the position before the mistake, with You to move. */
async function playOut(spot: WeakSpot) {
  const key = `${spot.game}:${spot.ply}`;
  starting.value = key;
  const whiteToMove = spot.fen.split(" ")[1] !== "b";
  try {
    // At the level nearest your rating, so the position is practised against someone you can learn from.
    if (!dameless.levels) await dameless.loadLevels().catch(() => {});
    const seat = playOutSeat(dameless.levels);
    const s = await games.start({
      white: whiteToMove ? "you" : seat,
      black: whiteToMove ? seat : "you",
      time: readClock(clock.value) ?? { type: "clock", initial_ms: 300_000, increment_ms: 3000 },
      from: { type: "position", fen: spot.fen, moves: [] },
    });
    await router.push({ path: "/", query: { game: String(s.id) } });
  } catch (e) {
    toast.show({
      title: "The game did not start",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  } finally {
    starting.value = null;
  }
}

const gameMoments = computed(() => {
  const m = props.profile.moments;
  return [
    m.most_accurate_game ? { label: "Most accurate game", g: m.most_accurate_game } : null,
    m.least_accurate_game ? { label: "Least accurate game", g: m.least_accurate_game } : null,
  ].filter((x) => x !== null);
});
</script>

<template>
  <div class="profile">
    <section v-if="profile.games" class="sec" aria-label="In figures">
      <dl class="figs">
        <div class="fig">
          <dt>Games</dt>
          <dd>
            {{ profile.games }}
            <small class="fig-sub">{{ profile.reviewed }} reviewed</small>
          </dd>
        </div>
        <div class="fig">
          <dt>Score</dt>
          <dd>
            {{ percent(profile.results.overall.score)
            }}<small v-if="profile.results.overall.score != null" class="unit">%</small>
          </dd>
        </div>
        <div class="fig">
          <dt><Term k="accuracy" /></dt>
          <dd>
            {{ percent(profile.accuracy.overall) }}<small v-if="profile.accuracy.overall != null" class="unit">%</small>
          </dd>
        </div>
        <div class="fig">
          <dt><Term k="acpl" /></dt>
          <dd>{{ figure(profile.acpl) }}</dd>
        </div>
        <div class="fig">
          <dt><Term k="performance" /></dt>
          <dd>{{ profile.opponents.performance == null ? "—" : Math.round(profile.opponents.performance) }}</dd>
        </div>
        <div class="fig">
          <dt>Win streak</dt>
          <dd>{{ profile.games ? profile.longest_win_streak : "—" }}</dd>
        </div>
      </dl>
    </section>

    <section v-if="profile.trend.length > 1" class="sec" aria-labelledby="p-trend">
      <div class="sec-head">
        <h2 id="p-trend" class="part-title">Accuracy and rating over time</h2>
        <p class="sec-note">Last {{ plural(profile.trend.length, "game") }}</p>
      </div>
      <TrendChart :points="profile.trend" :label="`Accuracy and rating over the last ${profile.trend.length} games`" />
    </section>

    <section v-if="reviewed" class="sec" aria-labelledby="p-acc">
      <div class="sec-head">
        <h2 id="p-acc" class="part-title">Accuracy by phase and speed</h2>
        <p class="sec-note">Percent</p>
      </div>
      <ul class="plain meters">
        <li v-for="row in [...phases, ...speeds]" :key="row.label" class="meter">
          <span class="meter-label">{{ row.label }}</span>
          <span
            class="meter-bar"
            role="img"
            :aria-label="`${row.label} accuracy ${row.value == null ? 'unknown' : percent(row.value)}`"
          >
            <span class="meter-fill" :style="{ inlineSize: `${Math.max(0, Math.min(100, row.value ?? 0))}%` }"></span>
          </span>
          <span class="meter-value mono">{{ percent(row.value) }}</span>
        </li>
      </ul>
    </section>

    <section v-if="profile.games" class="sec" aria-labelledby="p-res">
      <div class="sec-head">
        <h2 id="p-res" class="part-title">Results</h2>
        <p class="sec-note">Won-drawn-lost, score %</p>
      </div>
      <ul class="plain results">
        <li v-for="s in sides" :key="s.label" class="result">
          <span class="result-label">{{ s.label }}</span>
          <WdlBar :wdl="s.wdl" :label="s.label" />
          <span class="result-score mono">{{ percent(s.wdl.score) }}</span>
        </li>
      </ul>
      <p v-if="profile.opponents.average_rating" class="sec-note">
        Against opponents averaging {{ Math.round(profile.opponents.average_rating) }}.
      </p>
    </section>

    <section v-if="reviewed" class="sec" aria-labelledby="p-moves">
      <div class="sec-head">
        <h2 id="p-moves" class="part-title">Moves per game</h2>
        <p class="sec-note">Over {{ plural(profile.reviewed, "reviewed game") }}</p>
      </div>
      <div class="classes-legend" aria-hidden="true">
        <p v-for="n in 2" :key="n" class="class-legend">
          <span class="class-rate">Per game</span><span class="class-total">Total</span>
        </p>
      </div>
      <ul class="plain classes">
        <li v-for="r in classRows" :key="r.c" class="class" :class="r.tone ? `rv-${r.tone}` : 'class--quiet'">
          <span class="class-glyph" aria-hidden="true">{{ r.glyph }}</span>
          <span class="class-label">{{ r.label }}</span>
          <span class="class-rate mono">{{ r.perGame.toFixed(2) }}</span>
          <span class="class-total mono">{{ r.total }}</span>
        </li>
      </ul>
    </section>

    <section v-if="profile.openings.length" class="sec" aria-labelledby="p-open">
      <h2 id="p-open" class="part-title">Openings</h2>
      <ETable caption="Openings played" hide-caption dense>
        <thead>
          <tr>
            <th scope="col">Opening</th>
            <th scope="col" class="e-num">Games</th>
            <th scope="col" class="open-bar">Won · drawn · lost</th>
            <th scope="col" class="e-num">Score, %</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="o in profile.openings" :key="o.name">
            <th scope="row">{{ o.name }}</th>
            <td class="e-num">{{ o.results.games }}</td>
            <td class="open-bar"><WdlBar :wdl="o.results" :label="o.name" /></td>
            <td class="e-num">{{ percent(o.results.score, 0) }}</td>
          </tr>
        </tbody>
      </ETable>
    </section>

    <section v-if="reviewed && profile.time.games" class="sec" aria-labelledby="p-time">
      <div class="sec-head">
        <h2 id="p-time" class="part-title">Time per move</h2>
        <p class="sec-note">From the clocks of {{ plural(profile.time.games, "game") }}</p>
      </div>
      <dl class="figs figs--small">
        <div class="fig">
          <dt>Average</dt>
          <dd>{{ seconds(profile.time.average_ms) }}</dd>
        </div>
        <div class="fig">
          <dt>Opening</dt>
          <dd>{{ seconds(profile.time.opening_ms) }}</dd>
        </div>
        <div class="fig">
          <dt>Middlegame</dt>
          <dd>{{ seconds(profile.time.middlegame_ms) }}</dd>
        </div>
        <div class="fig">
          <dt>Endgame</dt>
          <dd>{{ seconds(profile.time.endgame_ms) }}</dd>
        </div>
        <div class="fig">
          <dt>Time-trouble blunders</dt>
          <dd>
            {{ profile.time.time_trouble_blunders }}
            <small>{{ profile.time.time_trouble_blunders_per_game.toFixed(2) }} a game</small>
          </dd>
        </div>
      </dl>
    </section>

    <section
      v-if="profile.moments.best.length || profile.moments.blunders.length || gameMoments.length"
      class="sec"
      aria-labelledby="p-moments"
    >
      <h2 id="p-moments" class="part-title">Best moments and worst blunders</h2>
      <ul class="plain moments">
        <li v-for="m in profile.moments.best" :key="`b${m.game}-${m.ply}`" class="moment">
          <span class="moment-mark" :class="`rv-${m.classification}`">{{ markOf(m.classification).glyph }}</span>
          <ELink :link-component="RouterLink" :to="boardTo(m.game, m.ply)" class="mono">
            {{ momentText(m) }}
          </ELink>
          <span class="moment-text">{{ momentNote(m) }}</span>
          <span class="moment-meta mono">{{ swing(m) }}</span>
        </li>
        <li v-for="m in profile.moments.blunders" :key="`x${m.game}-${m.ply}`" class="moment">
          <span class="moment-mark" :class="`rv-${m.classification}`">{{ markOf(m.classification).glyph }}</span>
          <ELink :link-component="RouterLink" :to="boardTo(m.game, m.ply)" class="mono">
            {{ momentText(m) }}
          </ELink>
          <span class="moment-text">Blunder against {{ m.opponent }}</span>
          <span class="moment-meta mono">{{ swing(m) }}</span>
        </li>
        <li v-for="g in gameMoments" :key="g.label" class="moment">
          <span class="moment-mark"></span>
          <ELink :link-component="RouterLink" :to="boardTo(g.g.game)" class="moment-move">{{ g.label }}</ELink>
          <span class="moment-text">
            {{ resultText(g.g.outcome) }} against {{ g.g.opponent }}, {{ shortDate(g.g.played_at) }}
          </span>
          <span class="moment-meta mono">{{ percent(g.g.accuracy) }}</span>
        </li>
      </ul>
    </section>

    <section v-if="isYou && profile.weak_spots.length" class="sec" aria-labelledby="p-weak">
      <div class="sec-head">
        <h2 id="p-weak" class="part-title">Weak spots</h2>
        <p class="sec-note">Positions before your mistakes, themes that come back first</p>
      </div>
      <ul class="plain spots">
        <li v-for="s in profile.weak_spots" :key="`${s.game}-${s.ply}`" class="spot">
          <div class="spot-text">
            <span class="spot-line">
              <span class="mono">{{ Math.ceil(s.ply / 2) }}{{ s.ply % 2 ? "." : "…" }} {{ s.played }}</span>
              <span class="spot-better"
                >better <span class="mono">{{ s.best }}</span></span
              >
            </span>
            <span class="sec-note">
              {{ s.opening ?? "Unnamed opening" }}, {{ s.phase
              }}<template v-if="s.recurring > 1"> · comes up {{ s.recurring }} times</template>
              ·
              <ELink :link-component="RouterLink" :to="boardTo(s.game, s.ply)">see the game</ELink>
            </span>
          </div>
          <span class="spot-swing mono">−{{ s.swing.toFixed(0) }}%</span>
          <EButton variant="outline" size="sm" :loading="starting === `${s.game}:${s.ply}`" @click="playOut(s)">
            Play this out
          </EButton>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped>
.profile {
  display: grid;
  min-width: 0;
}

.meters,
.results {
  gap: var(--e-gap-tight);
}

.meter {
  display: grid;
  grid-template-columns: 6.5rem minmax(0, 1fr) 3.5rem;
  align-items: center;
  gap: var(--e-gap-snug);
}

.meter-bar {
  display: block;
  block-size: 0.5rem;
  overflow: hidden;
  border-radius: var(--e-radius-sm);
  background: var(--e-border);
}

.meter-fill {
  display: block;
  block-size: 100%;
  background: var(--e-ink);
}

.meter-value,
.result-score {
  text-align: end;
}

.result {
  display: grid;
  grid-template-columns: 6.5rem minmax(0, 1fr) 3.5rem;
  align-items: center;
  gap: var(--e-gap-snug);
}

/* A figure with a note under it is taller than its neighbours; the neighbours' numbers stay on the same line. */
.fig {
  align-content: start;
}

.fig-sub {
  display: block;
  margin-block-start: 0.125rem;
}

.unit {
  margin-inline-start: 0.1875rem;
}

.classes-legend {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0 var(--e-gap-base);
  margin-block-end: calc(-1 * var(--e-gap-snug));
  color: var(--e-muted);
  font: var(--e-type-small);
}

.class-legend {
  display: grid;
  grid-template-columns: 1.5rem minmax(0, 1fr) 3rem 2.5rem;
  gap: var(--e-gap-tight);
  margin: 0;
  padding-block-end: 0.1875rem;
  border-block-end: var(--line);
}

.class-legend span {
  text-align: end;
  white-space: nowrap;
}

.class-legend span:first-child {
  grid-column: 3;
}

.classes {
  grid-auto-flow: column;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  grid-template-rows: repeat(6, auto);
  gap: 0 var(--e-gap-base);
}

.class {
  display: grid;
  grid-template-columns: 1.5rem minmax(0, 1fr) 3rem 2.5rem;
  align-items: baseline;
  gap: var(--e-gap-tight);
  padding-block: 0.1875rem;
  border-block-end: var(--line);
}

.class-glyph {
  color: var(--rv);
  font-weight: 800;
}

.class-label::before {
  content: none;
}

.class:not(.class--quiet) .class-label {
  color: var(--rv);
}

.class--quiet .class-label {
  color: var(--e-muted);
}

.class-rate,
.class-total {
  text-align: end;
}

.class-total {
  color: var(--e-muted);
}

.open-bar {
  inline-size: 40%;
}

.moments {
  gap: 0;
}

.moment {
  display: grid;
  grid-template-columns: 1.5rem auto minmax(0, 1fr) auto;
  align-items: baseline;
  gap: var(--e-gap-tight) var(--e-gap-snug);
  padding-block: 0.3125rem;
  border-block-end: var(--line);
}

.moment-mark {
  color: var(--rv);
  font-weight: 800;
}

.moment-text {
  min-width: 0;
  overflow: hidden;
  color: var(--e-muted);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.moment-meta {
  color: var(--e-muted);
  font-size: 0.8125rem;
}

.spots {
  gap: 0;
}

.spot {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  align-items: center;
  gap: var(--e-gap-snug);
  padding-block: var(--e-gap-snug);
  border-block-end: var(--line);
}

.spot-text {
  display: grid;
  gap: 0.125rem;
  min-width: 0;
}

.spot-line {
  display: flex;
  flex-wrap: wrap;
  gap: 0 var(--e-gap-base);
}

.spot-better {
  color: var(--e-muted);
}

.spot-swing {
  color: var(--rv-blunder);
}

@media (width < 36rem) {
  .classes-legend {
    grid-template-columns: minmax(0, 1fr);
  }

  .class-legend:last-child {
    display: none;
  }

  .classes {
    grid-auto-flow: row;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: none;
  }

  .moment {
    grid-template-columns: 1.5rem auto minmax(0, 1fr);
  }

  .moment-meta {
    display: none;
  }

  .spot {
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .spot .spot-swing {
    display: none;
  }

  .meter,
  .result {
    grid-template-columns: 5.5rem minmax(0, 1fr) 3rem;
  }

  .open-bar {
    display: none;
  }
}
</style>
