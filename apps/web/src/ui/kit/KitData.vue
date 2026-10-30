<script setup lang="ts">
import { ECheckbox } from "@eos-ui/vue/components/ECheckbox";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EKbd } from "@eos-ui/vue/components/EKbd";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETabs } from "@eos-ui/vue/components/ETabs";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { ETextarea } from "@eos-ui/vue/components/ETextarea";
import { PhMagnifyingGlass, PhTray } from "@phosphor-icons/vue";
import { ref } from "vue";
import UiBadge from "../UiBadge.vue";
import UiEmpty from "../UiEmpty.vue";
import UiField from "../UiField.vue";
import UiKeyValue from "../UiKeyValue.vue";
import UiKv from "../UiKv.vue";
import UiPlayer from "../UiPlayer.vue";
import UiSection from "../UiSection.vue";
import UiSeg from "../UiSeg.vue";
import UiTable from "../UiTable.vue";
import UiToolbar from "../UiToolbar.vue";
import { botSeat, levelColumns, levelRows, type LevelRow } from "./data";
import UiSelect from "../../ui/UiSelect.vue";

const open = ref(true);
const selected = ref("3");
const picked = ref<string | null>(null);
const expanded = ref<string[]>(["2"]);
const mode = ref("play");
const title = ref("");
const speed = ref("blitz");
const hash = ref(64);
const rated = ref(true);
const tab = ref("one");
const note = ref("");
const engine = ref("stockfish");

const kvItems = [
  { label: "Cores", value: 15 },
  { label: "Analysis engine", value: "C:\\tools\\stockfish\\stockfish.exe", mono: true, copy: true },
  { label: "Data", value: "C:\\Users\\dame\\AppData\\Local\\chess", mono: true, copy: true },
  { label: "Engines", value: "2 engines, 182 MB" },
  { label: "Status", value: "Healthy", tone: "win" as const },
];

const wideColumns = [
  { key: "name", label: "Opening" },
  { key: "games", label: "Games", fig: true },
  { key: "won", label: "Won", fig: true },
  { key: "drawn", label: "Drawn", fig: true },
  { key: "lost", label: "Lost", fig: true },
  { key: "score", label: "Score", fig: true },
  { key: "acpl", label: "ACPL", fig: true },
];
const wideRows = [
  { name: "Sicilian Defense: Najdorf Variation", games: 412, won: 210, drawn: 62, lost: 140, score: 57.2, acpl: 31 },
  { name: "Italian Game", games: 228, won: 121, drawn: 40, lost: 67, score: 61.8, acpl: 28 },
  { name: "Queen's Gambit Declined", games: 96, won: 40, drawn: 18, lost: 38, score: 51.0, acpl: 35 },
];
const names = (r: LevelRow) => String(r.level);
const doubled = [...levelRows, ...levelRows].map((r, id) => ({ ...r, id }));
</script>

<template>
  <UiSection id="section" title="UiSection">
    <template #note>The one section frame: a hairline above, the title, an aside, a note, then content.</template>
    <UiSection title="Level 1 with an aside and a note" :level="1">
      <template #aside>
        <UiSeg
          v-model="speed"
          label="Speed"
          size="sm"
          :options="[
            { value: 'blitz', label: 'Blitz' },
            { value: 'rapid', label: 'Rapid' },
          ]"
        />
        <EButton variant="quiet" size="sm">Link</EButton>
      </template>
      <template #note>One muted line under the title says what the part is for.</template>
      <p class="kit-p">
        Content sits <code class="mono">--sp-2</code> under the header. A section after this one gets a hairline above
        it.
      </p>
    </UiSection>
    <UiSection title="Level 2 has no hairline" :level="2">
      <p class="kit-p">A part of a part: a quieter title, no rule.</p>
    </UiSection>
    <UiSection v-model:open="open" title="Collapsible (click the title)" collapsible>
      <p class="kit-p">Hidden and shown with the title. The caret turns.</p>
    </UiSection>
    <div class="kit-narrow">
      <UiSection title="In a narrow container" :level="1">
        <template #aside><EButton variant="outline" size="sm">Aside wraps below</EButton></template>
        <p class="kit-p">Under 30rem wide the aside drops beneath the title.</p>
      </UiSection>
    </div>
  </UiSection>

  <UiSection id="keyvalue" title="UiKeyValue">
    <template #note>Facts as a list. Labels left, values flowing. Stacks in a narrow container.</template>
    <UiKeyValue :items="kvItems" />
    <UiKeyValue :items="kvItems.slice(0, 4)" :columns="2" dense />
    <UiKeyValue>
      <UiKv label="Slot rows">written as children</UiKv>
      <UiKv label="Mono" mono>5+3 · e2e4 · 9e3a5e41</UiKv>
    </UiKeyValue>
    <UiKeyValue
      inline
      :items="[
        { label: 'Time control', value: '5+3' },
        { label: 'Where', value: 'local game' },
        { label: 'Rating', value: 'rated' },
      ]"
    />
    <div class="kit-narrow"><UiKeyValue :items="kvItems.slice(0, 3)" /></div>
  </UiSection>

  <UiSection id="table" title="UiTable">
    <template #note>
      28 px rows, hairlines, sentence-case head, figures mono at the end edge, first column strong.
    </template>
    <UiTable label="Levels" :columns="levelColumns" :rows="levelRows" row-key="level" />
    <UiSection title="Selectable rows (a @select makes them buttons)" :level="2">
      <UiTable
        label="Selectable levels"
        :columns="levelColumns"
        :rows="levelRows"
        :row-key="names"
        :selected="selected"
        @select="(r: LevelRow) => ((selected = String(r.level)), (picked = r.name))"
      />
      <p class="muted kit-p">{{ picked ? `Chose ${picked}` : "Click or press Enter on a row." }}</p>
    </UiSection>
    <UiSection title="Custom cells, expandable rows, a footer, edge padding" :level="2">
      <UiTable
        label="Levels with cells"
        :columns="levelColumns"
        :rows="levelRows.slice(0, 3)"
        row-key="level"
        :expanded="expanded"
        edge
        :dense="false"
        @select="
          (r: LevelRow) =>
            (expanded = expanded.includes(String(r.level))
              ? expanded.filter((k) => k !== String(r.level))
              : [...expanded, String(r.level)])
        "
      >
        <template #cell-name="{ row }">
          <span class="row"
            ><UiBadge tone="info">{{ row.level }}</UiBadge
            >{{ row.name }}</span
          >
        </template>
        <template #row-aside="{ row }">
          <span class="muted"
            >{{ row.name }} searches {{ row.nodes.toLocaleString("en-GB") }} nodes a move, picked from the best two
            lines.</span
          >
        </template>
        <template #foot>3 of 6 levels</template>
      </UiTable>
    </UiSection>
    <UiSection title="Loading, empty" :level="2">
      <div class="kit-pair">
        <UiTable label="Loading" :columns="levelColumns" :rows="[]" row-key="level" loading />
        <UiTable
          label="Empty"
          :columns="levelColumns"
          :rows="[]"
          row-key="level"
          empty="No levels yet. Play one to rate it."
        />
      </div>
    </UiSection>
    <UiSection title="In a narrow container: columns hide, a wide table scrolls with a fade" :level="2">
      <div class="kit-pair">
        <div class="kit-narrow kit-narrow--table">
          <UiTable label="Narrow levels" :columns="levelColumns" :rows="levelRows" row-key="level" />
        </div>
        <div class="kit-narrow kit-narrow--table">
          <UiTable label="Openings" :columns="wideColumns" :rows="wideRows" row-key="name" />
        </div>
      </div>
    </UiSection>
    <UiSection title="A sticky head inside a scrolling pane" :level="2">
      <div class="kit-scroll">
        <UiTable label="Sticky levels" :columns="levelColumns" :rows="doubled" row-key="id" sticky />
      </div>
    </UiSection>
  </UiSection>

  <UiSection id="empty" title="UiEmpty">
    <template #note>One sentence that teaches the next action, one button. Errors are red in the icon only.</template>
    <div class="kit-pair">
      <UiEmpty :icon="PhTray" title="No games yet" body="Play one against an engine and it shows up here.">
        <template #actions>
          <EButton size="sm">New game</EButton><EButton size="sm" variant="quiet">Import</EButton>
        </template>
      </UiEmpty>
      <UiEmpty tone="error" title="Could not load games" body="The app did not answer.">
        <template #actions><EButton size="sm" variant="outline">Try again</EButton></template>
      </UiEmpty>
    </div>
    <UiEmpty dense title="No drills yet." body="Review a game of yours and its mistakes become drills.">
      <template #actions><EButton size="sm" variant="quiet">Review games</EButton></template>
    </UiEmpty>
    <UiEmpty dense tone="error" title="Could not load the ladder.">
      <template #actions><EButton size="sm" variant="quiet">Try again</EButton></template>
    </UiEmpty>
  </UiSection>

  <UiSection id="toolbar" title="UiToolbar">
    <template #note>
      A row of controls a control tall, wrapping. The default slot at the start, #end at the other edge.
    </template>
    <UiToolbar>
      <UiSeg
        v-model="mode"
        label="Mode"
        :options="[
          { value: 'play', label: 'Play' },
          { value: 'explore', label: 'Explore' },
        ]"
      />
      <EButton variant="outline">Offer draw</EButton>
      <EButton variant="outline">Take back</EButton>
      <template #end><EButton variant="danger">Resign</EButton></template>
    </UiToolbar>
    <UiToolbar align="between" :wrap="false">
      <ETextField v-model="note" label="Search" hide-label placeholder="Search games" :icon="PhMagnifyingGlass" />
      <template #end><EKbd>Ctrl</EKbd><EKbd>K</EKbd></template>
    </UiToolbar>
    <div class="kit-scroll kit-scroll--short">
      <UiToolbar sticky label="Sticky">
        <UiSeg
          v-model="mode"
          label="Mode"
          size="sm"
          :options="[
            { value: 'play', label: 'Play' },
            { value: 'explore', label: 'Explore' },
          ]"
        /><span class="muted">Sticks to the top of this pane</span>
      </UiToolbar>
      <p v-for="n in 12" :key="n" class="kit-p muted">Scrolling content line {{ n }}</p>
    </div>
  </UiSection>

  <UiSection id="field" title="UiField and the eos controls under it">
    <template #note>
      The label above, the control, then a hint or the error in its place. eos controls take the app's heights.
    </template>
    <div class="kit-form">
      <UiField v-slot="{ id, describedby, invalid }" label="Title" :error="title ? undefined : 'Give it a title.'">
        <ETextField
          :id="id"
          v-model="title"
          label="Title"
          hide-label
          :aria-describedby="describedby"
          :invalid="invalid"
        />
      </UiField>
      <UiField v-slot="{ id }" label="Program" optional hint="The engine's executable on this machine.">
        <ETextField :id="id" label="Program" hide-label placeholder="C:\engines\berserk.exe" />
      </UiField>
      <UiField label="Time control" group hint="Minutes plus seconds a move.">
        <UiSeg
          v-model="speed"
          label="Time control"
          full
          :options="[
            { value: 'bullet', label: '1+0' },
            { value: 'blitz', label: '5+3' },
            { value: 'rapid', label: '10+5' },
            { value: 'custom', label: 'Custom…' },
          ]"
        />
      </UiField>
      <UiSelect
        v-model="engine"
        label="Engine"
        :options="[
          { value: 'stockfish', label: 'Stockfish' },
          { value: 'reckless', label: 'Reckless' },
        ]"
      />
      <UiField v-slot="{ id }" label="Hash" hint="In megabytes.">
        <ENumberField :id="id" v-model="hash" label="Hash" hide-label :min="1" />
      </UiField>
      <UiField v-slot="{ id }" label="Notes" optional>
        <ETextarea :id="id" v-model="note" label="Notes" hide-label :rows="2" />
      </UiField>
    </div>
    <div class="row">
      <ESwitch v-model="rated" label="Rated" />
      <ECheckbox v-model="rated" label="Casual" />
      <EButton>Primary</EButton>
      <EButton variant="outline">Outline</EButton>
      <EButton variant="quiet">Quiet</EButton>
      <EButton variant="danger">Danger</EButton>
      <EButton variant="primary" size="sm">Small</EButton>
      <EButton disabled>Disabled</EButton>
      <EButton loading>Loading</EButton>
    </div>
    <ETabs
      v-model="tab"
      label="Tabs"
      :items="[
        { id: 'one', label: 'One' },
        { id: 'two', label: 'Two' },
      ]"
    />
    <EDisclosure title="A disclosure"><p class="kit-p">Advanced options go in one.</p></EDisclosure>
  </UiSection>

  <UiSection id="utilities" title="Utilities">
    <template #note><code class="mono">.stack .row .muted .truncate .sr-only .mono .num .skel</code></template>
    <div class="row">
      <span class="skel kit-skel" /> <span class="skel kit-skel kit-skel--wide" /> <UiPlayer :seat="botSeat" />
      <span class="muted truncate kit-trunc">A long muted line that is cut with an ellipsis when it does not fit</span>
    </div>
  </UiSection>
</template>

<style scoped>
.kit-p {
  margin: 0;
}

.kit-narrow {
  max-width: 24rem;
  padding: var(--sp-2);
  outline: var(--e-stroke-hair) dashed var(--c-line-strong);
}

.kit-narrow--table {
  min-width: 0;
}

.kit-pair {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
  gap: var(--sp-4);
}

.kit-scroll {
  max-height: 14rem;
  overflow-y: auto;
  outline: var(--e-stroke-hair) solid var(--c-line-strong);
}

.kit-scroll--short {
  max-height: 9rem;
  padding-inline: var(--sp-2);
}

.kit-form {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
  gap: var(--sp-3);
}

.kit-skel {
  width: 6rem;
  height: 1rem;
}

.kit-skel--wide {
  width: 12rem;
}

.kit-trunc {
  max-width: 14rem;
}
</style>
