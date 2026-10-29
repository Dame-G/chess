import { createApp, h, nextTick, reactive } from "vue";
import { createEos } from "@eos-ui/vue/plugin";
import { afterEach, describe, expect, it } from "vitest";
import UiConfirm from "../../src/ui/UiConfirm.vue";
import UiDialog from "../../src/ui/UiDialog.vue";
import { useConfirm } from "../../src/ui/useConfirm";
import { key, text } from "./mount";

/** A dialog is portaled out of the component, so these look in the whole document, not the mount point. */
const q = (selector: string) => document.querySelector<HTMLElement>(selector);
const wait = (ms = 20) => new Promise((r) => setTimeout(r, ms));

let cleanup: (() => void) | undefined;
afterEach(() => {
  cleanup?.();
  cleanup = undefined;
  document.body.innerHTML = "";
});

function mountApp(root: () => ReturnType<typeof h>) {
  const el = document.createElement("div");
  el.className = "e-root";
  document.body.append(el);
  const app = createApp({ render: root });
  app.use(createEos({ locale: "en-US", applyTheme: false }));
  app.mount(el);
  cleanup = () => app.unmount();
}

describe("UiDialog", () => {
  it("shows its title, body and footer with Cancel before the action", async () => {
    const state = reactive({ open: true });
    mountApp(() =>
      h(
        UiDialog,
        {
          open: state.open,
          "onUpdate:open": (v: boolean) => (state.open = v),
          title: "New idea",
          confirmLabel: "Queue idea",
        },
        { default: () => h("input", { class: "field" }), "footer-start": () => "Give it a title." },
      ),
    );
    await wait();
    expect(text(q(".e-dialog-title"))).toBe("New idea");
    const buttons = [...document.querySelectorAll(".e-dialog-foot button")].map((b) => text(b));
    expect(buttons).toEqual(["Cancel", "Queue idea"]);
    expect(text(q(".ui-dialog-hint"))).toBe("Give it a title.");
    expect(q('[role="dialog"]')?.getAttribute("aria-modal")).toBe("true");
  });

  it("puts focus on the first field, not on the close button", async () => {
    mountApp(() =>
      h(
        UiDialog,
        { open: true, title: "Edit", confirmLabel: "Save" },
        { default: () => [h("input", { class: "one" }), h("input", { class: "two" })] },
      ),
    );
    await wait(50);
    expect(document.activeElement).toBe(q(".one"));
  });

  it("confirms on Enter from a field, never from Cancel, and not when it cannot", async () => {
    let confirmed = 0;
    const state = reactive({ disabled: false });
    mountApp(() =>
      h(
        UiDialog,
        {
          open: true,
          title: "Edit",
          confirmLabel: "Save",
          confirmDisabled: state.disabled,
          onConfirm: () => confirmed++,
        },
        { default: () => h("input", { class: "field" }) },
      ),
    );
    await wait();
    key(q(".field")!, "Enter");
    expect(confirmed).toBe(1);
    key(q(".ui-dialog-cancel")!, "Enter");
    expect(confirmed).toBe(1);
    state.disabled = true;
    await nextTick();
    key(q(".field")!, "Enter");
    expect(confirmed).toBe(1);
  });

  it("cancels with the Cancel button: it closes and says so", async () => {
    const state = reactive({ open: true, cancelled: 0 });
    mountApp(() =>
      h(UiDialog, {
        open: state.open,
        "onUpdate:open": (v: boolean) => (state.open = v),
        title: "Edit",
        confirmLabel: "Save",
        onCancel: () => state.cancelled++,
      }),
    );
    await wait();
    q(".ui-dialog-cancel")?.click();
    await nextTick();
    expect(state.open).toBe(false);
    expect(state.cancelled).toBe(1);
  });

  it("does not close on confirm: the caller does, when the work is done", async () => {
    const state = reactive({ open: true });
    mountApp(() =>
      h(UiDialog, {
        open: state.open,
        "onUpdate:open": (v: boolean) => (state.open = v),
        title: "Edit",
        confirmLabel: "Save",
      }),
    );
    await wait();
    q(".ui-dialog-confirm")?.click();
    await nextTick();
    expect(state.open).toBe(true);
  });

  it("is red when it is a danger, and a busy confirm ignores clicks", async () => {
    let confirmed = 0;
    mountApp(() =>
      h(UiDialog, {
        open: true,
        title: "Delete?",
        confirmLabel: "Delete",
        danger: true,
        busy: true,
        onConfirm: () => confirmed++,
      }),
    );
    await wait();
    const button = q(".ui-dialog-confirm");
    expect(button?.className).toContain("e-btn--danger");
    expect(button?.getAttribute("aria-busy")).toBe("true");
    button?.click();
    expect(confirmed).toBe(0);
    expect(q(".ui-dialog-cancel")?.hasAttribute("disabled")).toBe(true);
  });

  it("is an alert dialog when it needs an answer, landing on Cancel", async () => {
    mountApp(() => h(UiDialog, { open: true, alert: true, title: "Resign?", confirmLabel: "Resign", danger: true }));
    await wait(50);
    expect(q('[role="alertdialog"]')).not.toBeNull();
    expect(document.activeElement).toBe(q(".ui-dialog-cancel"));
  });

  it("is as wide as its size says", async () => {
    mountApp(() => h(UiDialog, { open: true, title: "Wide", size: "lg" }));
    await wait();
    expect(q(".ui-dialog")?.style.getPropertyValue("--_w")).toBe("40rem");
  });
});

describe("UiConfirm", () => {
  function host() {
    const asked = { ask: undefined as ReturnType<typeof useConfirm> | undefined };
    mountApp(() =>
      h("div", [
        h(UiConfirm),
        h({
          setup() {
            asked.ask = useConfirm();
            return () => null;
          },
        }),
      ]),
    );
    return asked;
  }

  it("asks the question and answers yes from the action", async () => {
    const asked = host();
    await nextTick();
    const answer = asked.ask!({
      title: "Resign this game?",
      body: "You lose the game.",
      confirmLabel: "Resign",
      danger: true,
    });
    await wait(30);
    expect(text(q(".e-dialog-title"))).toBe("Resign this game?");
    expect(text(q(".ui-confirm-body"))).toBe("You lose the game.");
    expect(q(".ui-dialog-confirm")?.className).toContain("e-btn--danger");
    q(".ui-dialog-confirm")?.click();
    expect(await answer).toBe(true);
  });

  it("answers no from Cancel and from the close button", async () => {
    const asked = host();
    await nextTick();
    const first = asked.ask!({ title: "Push?", confirmLabel: "Push" });
    await wait(30);
    q(".ui-dialog-cancel")?.click();
    expect(await first).toBe(false);
    await wait(30);
    const second = asked.ask!({ title: "Promote?", confirmLabel: "Promote" });
    await wait(30);
    q(".e-dialog-head button")?.click();
    expect(await second).toBe(false);
  });
});
