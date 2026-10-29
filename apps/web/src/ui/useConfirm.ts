import { shallowRef } from "vue";

/** What a confirmation says: the action as a question, a sentence of consequence, the action's own name. */
export interface ConfirmOptions {
  /** The action as a question, on one line: "Resign this game?". */
  title: string;
  /** One sentence of what happens. */
  body?: string;
  /** The action, as a verb: "Resign", "Promote to v15". Not "OK". */
  confirmLabel: string;
  cancelLabel?: string;
  /** A destructive action: the button is red. */
  danger?: boolean;
}

interface Pending extends ConfirmOptions {
  resolve: (answer: boolean) => void;
}

/** The confirmation on screen, and the ones waiting behind it. Read by the one `UiConfirm` host. */
export const current = shallowRef<Pending | null>(null);
const waiting: Pending[] = [];
let hosts = 0;

/** The host calls this while it is mounted, so a call with no host is refused loudly instead of hanging. */
export function registerHost(): () => void {
  hosts++;
  return () => {
    hosts--;
  };
}

function next() {
  current.value = waiting.shift() ?? null;
}

/** The host answers the one on screen. */
export function answer(yes: boolean) {
  const pending = current.value;
  if (!pending) return;
  pending.resolve(yes);
  next();
}

/** Asks and waits: true when the person confirmed, false when they cancelled or pressed Escape. One at a time. */
function ask(options: ConfirmOptions): Promise<boolean> {
  if (hosts === 0) {
    console.warn("useConfirm: no <UiConfirm /> is mounted, so the question was not asked.");
    return Promise.resolve(false);
  }
  return new Promise((resolve) => {
    const pending = { ...options, resolve };
    if (current.value) waiting.push(pending);
    else current.value = pending;
  });
}

/** `const confirm = useConfirm(); if (await confirm({ title: "Resign this game?", confirmLabel: "Resign", danger: true })) …` */
export function useConfirm(): (options: ConfirmOptions) => Promise<boolean> {
  return ask;
}
