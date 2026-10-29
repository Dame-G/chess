<script setup lang="ts">
import { EButton } from "@eos-ui/vue/components/EButton";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { ref } from "vue";
import UiDialog from "../UiDialog.vue";
import UiEmpty from "../UiEmpty.vue";
import UiField from "../UiField.vue";
import UiKeyValue from "../UiKeyValue.vue";
import UiLive from "../UiLive.vue";
import UiPanelHead from "../UiPanelHead.vue";
import UiSection from "../UiSection.vue";
import UiSheetPage, { type SheetTab } from "../UiSheetPage.vue";
import UiStat from "../UiStat.vue";
import UiStats from "../UiStats.vue";
import UiTable from "../UiTable.vue";
import UiToolbar from "../UiToolbar.vue";
import { useConfirm } from "../useConfirm";
import { levelColumns, levelRows, liveGame, lostGame, strangersGame, wonGame } from "./data";

// Dialogs
const basic = ref(false);
const title = ref("");
const busy = ref(false);
const danger = ref(false);
const wide = ref(false);
const long = ref(false);
const saved = ref("");

function save() {
  busy.value = true;
  setTimeout(() => {
    busy.value = false;
    saved.value = `Queued "${title.value}"`;
    basic.value = false;
    title.value = "";
  }, 800);
}

const confirm = useConfirm();
const answered = ref("");
async function resign() {
  const yes = await confirm({
    title: "Resign this game?",
    body: "You lose the game.",
    confirmLabel: "Resign",
    danger: true,
  });
  answered.value = yes ? "Resigned" : "Kept playing";
}
async function promote() {
  const yes = await confirm({
    title: "Promote to v15?",
    body: "The champion becomes v15. Nothing is pushed.",
    confirmLabel: "Promote to v15",
  });
  answered.value = yes ? "Promoted" : "Left as it was";
}

// Sheets
const tab = ref("overview");
const asPage = ref(false);
const tabs: SheetTab[] = [
  { id: "overview", label: "Overview" },
  { id: "games", label: "Games", count: 3708 },
  { id: "openings", label: "Openings" },
  { id: "drills", label: "Drills", count: 12 },
];
</script>

<template>
  <UiSection id="panelhead" title="UiPanelHead">
    <template #note>
      The game card at the top of the panel: who plays whom, the result between, one muted line.
    </template>
    <div class="kit-heads">
      <UiPanelHead :game="liveGame" mode="play">
        <template #status><UiLive pulse="1" /> Your move</template>
        <template #actions>
          <EButton variant="quiet" size="sm">Offer draw</EButton><EButton variant="quiet" size="sm">Resign</EButton>
        </template>
      </UiPanelHead>
      <UiPanelHead :game="wonGame" mode="review" />
      <UiPanelHead :game="lostGame" mode="review" />
      <UiPanelHead :game="strangersGame" mode="watch" />
      <UiPanelHead :game="null" mode="explore" />
      <UiEmpty dense title="The game: null draws nothing (free analysis)." />
    </div>
  </UiSection>

  <UiSection id="dialog" title="UiDialog and UiConfirm">
    <template #note>
      Title on one line, Cancel at the start, the one primary at the end, Enter confirms, focus lands on the first
      field. Under 40rem it is a bottom sheet.
    </template>
    <UiToolbar>
      <EButton variant="outline" @click="basic = true">New idea (form)</EButton>
      <EButton variant="outline" @click="wide = true">Large</EButton>
      <EButton variant="outline" @click="long = true">Long title</EButton>
      <EButton variant="outline" @click="danger = true">Danger</EButton>
      <EButton variant="outline" @click="resign">Resign (confirm)</EButton>
      <EButton variant="outline" @click="promote">Promote (confirm)</EButton>
    </UiToolbar>
    <p class="muted kit-p" role="status">{{ saved || answered || "Nothing chosen yet." }}</p>

    <UiDialog
      v-model:open="basic"
      title="New idea"
      confirm-label="Queue idea"
      :busy="busy"
      :confirm-disabled="!title"
      @confirm="save"
    >
      <div class="stack">
        <UiField v-slot="{ id }" label="Title">
          <ETextField :id="id" v-model="title" label="Title" hide-label />
        </UiField>
        <UiField v-slot="{ id }" label="Hypothesis" optional hint="What it should do, and why.">
          <ETextField :id="id" label="Hypothesis" hide-label />
        </UiField>
      </div>
      <template #footer-start>{{ title ? "" : "Give it a title." }}</template>
    </UiDialog>

    <UiDialog v-model:open="wide" size="lg" title="Bot rules" confirm-label="Save">
      <UiKeyValue
        :items="[
          { label: 'Version', value: 'Champion' },
          { label: 'Takes', value: 'Rated and casual' },
          { label: 'Speeds', value: 'Bullet, blitz, rapid' },
        ]"
      />
    </UiDialog>

    <UiDialog
      v-model:open="long"
      size="sm"
      title="A title that is far too long to fit on one line of a small dialog and so is cut"
      confirm-label="OK"
    >
      <p class="kit-p">The title stays on one line and ends in an ellipsis.</p>
    </UiDialog>

    <UiDialog
      v-model:open="danger"
      size="sm"
      title="Delete this dataset?"
      danger
      confirm-label="Delete dataset"
      @confirm="danger = false"
    >
      <p class="kit-p">The 4.2 GB of positions are removed from this machine.</p>
    </UiDialog>
  </UiSection>

  <UiSection id="sheet" title="UiSheetPage">
    <template #note>
      The frame of every sheet: a header that stays, a hero strip that scrolls away, tabs that stick, content that
      scrolls. Previews here are contained; the real one docks beside the board and covers the window under 75rem.
    </template>
    <div class="kit-sheets">
      <div class="kit-stage">
        <UiSheetPage
          v-model:tab="tab"
          contained
          :sync="false"
          title="You"
          subtitle="dame_g · Lichess"
          :tabs="tabs"
          @close="() => {}"
          @back="() => {}"
        >
          <template #actions><EButton variant="outline" size="sm">Review all</EButton></template>
          <template #lead>
            <UiStats :columns="3">
              <UiStat label="Rating" :value="1840" size="xl" /><UiStat label="Games" :value="3708" /><UiStat
                label="Accuracy"
                :value="81.3"
                unit="%"
              />
            </UiStats>
          </template>
          <template #tab-overview>
            <UiSection title="Results">
              <UiStats dense>
                <UiStat label="Won" :value="214" /><UiStat label="Drawn" :value="62" /><UiStat
                  label="Lost"
                  :value="140"
                />
              </UiStats>
            </UiSection>
            <UiSection title="Levels">
              <UiTable label="Levels" :columns="levelColumns" :rows="levelRows" row-key="level" />
            </UiSection>
            <UiSection title="Notes">
              <p class="kit-p">Scroll this pane: the header and the tabs stay.</p>
              <p v-for="n in 14" :key="n" class="kit-p muted">Line {{ n }}</p>
            </UiSection>
          </template>
          <template #default="{ tab: t }">
            <UiSection :title="`The ${t} tab`"><p class="kit-p">Another tab's content.</p></UiSection>
          </template>
        </UiSheetPage>
      </div>
      <div class="kit-stage">
        <UiSheetPage contained :sync="false" wide title="Compare" subtitle="You · Dameless v14" @close="() => {}">
          <UiSection title="One tab, so no tab row">
            <p class="kit-p">A single-tab sheet hides its tab row.</p>
          </UiSection>
        </UiSheetPage>
      </div>
      <div class="kit-stage">
        <UiSheetPage contained :sync="false" title="Dameless" loading :tabs="tabs" @close="() => {}" />
      </div>
      <div class="kit-stage">
        <UiSheetPage
          contained
          :sync="false"
          title="Dameless"
          error="The app did not answer."
          :tabs="tabs"
          @close="() => {}"
          @retry="() => {}"
        />
      </div>
    </div>
    <UiToolbar>
      <EButton variant="outline" @click="asPage = true"> Open as the real thing (window-wide below 75rem) </EButton>
    </UiToolbar>
    <div v-if="asPage" class="kit-dock">
      <UiSheetPage
        :sync="false"
        title="Settings"
        subtitle="Lichess, the machine, the theme"
        @close="asPage = false"
        @back="asPage = false"
      >
        <UiSection title="This machine">
          <UiKeyValue
            :items="[
              { label: 'Cores', value: 15 },
              { label: 'Data', value: 'C:\\dameless', mono: true, copy: true },
            ]"
          />
        </UiSection>
      </UiSheetPage>
    </div>
  </UiSection>
</template>

<style scoped>
.kit-p {
  margin: 0;
}

.kit-heads {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(20rem, 1fr));
  gap: var(--sp-3);
}

.kit-heads > * {
  outline: var(--e-stroke-hair) solid var(--c-line);
}

.kit-sheets {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 22rem), 1fr));
  gap: var(--sp-4);
}

/* Stands in for the app's workspace: a positioned box the sheet docks to the right of. */
.kit-dock {
  position: fixed;
  inset: var(--h-header) 0 0;
  z-index: var(--e-z-raised);
  pointer-events: none;
}

.kit-dock > :deep(*) {
  pointer-events: auto;
}

.kit-stage {
  position: relative;
  height: 34rem;
  overflow: hidden;
  background: var(--c-page);
  outline: var(--e-stroke-hair) solid var(--c-line-strong);
}
</style>
