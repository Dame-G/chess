import { onBeforeUnmount, watch, type WatchSource } from "vue";

const reduced = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : undefined;

/**
 * Keeps the current move (the element marked `aria-current`) in view inside a scrolling list. One read and one write
 * per frame however fast the moves change, a little room kept past the edge, and a short glide for a near move where
 * a far one jumps. In a virtual list the current move may not be drawn yet: `reveal` then scrolls to it.
 */
export function followCurrent(list: () => HTMLElement | undefined, source: WatchSource, reveal?: () => void) {
  let frame = 0;
  function settle() {
    frame = 0;
    const box = list();
    if (!box) return;
    const item = box.querySelector<HTMLElement>("[aria-current]");
    if (!item) {
      reveal?.();
      return;
    }
    const rows = box.getBoundingClientRect();
    const at = item.getBoundingClientRect();
    const room = at.height * 1.5;
    let by = 0;
    if (at.top < rows.top + room) by = at.top - rows.top - room;
    else if (at.bottom > rows.bottom - room) by = at.bottom - rows.bottom + room;
    if (!by) return;
    const near = Math.abs(by) < box.clientHeight * 0.6 && !reduced?.matches;
    box.scrollBy({ top: by, behavior: near ? "smooth" : "instant" });
  }
  watch(
    source,
    () => {
      if (!frame) frame = requestAnimationFrame(settle);
    },
    { immediate: true },
  );
  onBeforeUnmount(() => cancelAnimationFrame(frame));
}

/** What the arrow keys do to the board: a move back or forward, to the start, to the end. */
export type Step = "back" | "forward" | "start" | "end";
const STEPS: Record<string, Step> = { ArrowLeft: "back", ArrowRight: "forward", Home: "start", End: "end" };

/**
 * The step a key press asks for, if any. A move list is a list of cells whose own arrow keys stay inside a row; the
 * board's steps go on past its ends, so the lists hand these four keys to the board instead (as every other control
 * does), and the board's move stays the one in focus.
 */
export function stepOf(e: KeyboardEvent): Step | undefined {
  return e.altKey || e.ctrlKey || e.metaKey || e.shiftKey ? undefined : STEPS[e.key];
}

/** Puts focus on the current move once it is drawn (a few frames, for a virtual list that scrolls to it first). */
export function focusCurrent(list: () => HTMLElement | undefined) {
  let tries = 6;
  const look = () => {
    const item = list()?.querySelector<HTMLElement>("[aria-current]");
    if (item) item.focus({ preventScroll: true });
    else if (tries-- > 0) requestAnimationFrame(look);
  };
  requestAnimationFrame(look);
}
