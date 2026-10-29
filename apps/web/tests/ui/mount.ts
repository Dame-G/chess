import { createApp, h, nextTick, reactive, type App, type Component } from "vue";
import { createPinia } from "pinia";
import { createMemoryHistory, createRouter, type Router } from "vue-router";

export interface Mounted {
  el: HTMLElement;
  app: App;
  router: Router;
  /** The props the component was mounted with; change one and call `flush()` to see it re-render. */
  props: Record<string, unknown>;
  flush: () => Promise<void>;
  unmount: () => void;
}

type Slots = Record<string, (scope: never) => unknown>;

/** Mounts a component into the document with a memory router (at `at`) and pinia, as the app has them, and no eos plugin. */
export async function mount(
  component: Component,
  props: Record<string, unknown> = {},
  slots: Slots = {},
  at = "/",
): Promise<Mounted> {
  const el = document.createElement("div");
  document.body.append(el);
  const state = reactive({ ...props });
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: { render: () => null } },
      { path: "/:rest(.*)*", component: { render: () => null } },
    ],
  });
  const app = createApp({ render: () => h(component, state, slots as never) });
  app.use(createPinia()).use(router);
  await router.push(at);
  await router.isReady();
  app.mount(el);
  await nextTick();
  return {
    el,
    app,
    router,
    props: state,
    flush: () => nextTick(),
    unmount() {
      app.unmount();
      el.remove();
    },
  };
}

/** Text without the whitespace a template leaves between tags. */
export const text = (el: Element | null | undefined) => (el?.textContent ?? "").replace(/\s+/g, " ").trim();

/** A key press on an element, as the browser would send it. */
export function key(el: Element, k: string, init: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...init });
  el.dispatchEvent(event);
  return event;
}
