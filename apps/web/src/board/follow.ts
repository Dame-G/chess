import { onBeforeUnmount, watch, type Ref, type WatchSource } from "vue";

const reduced = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : undefined;

/**
 * Keeps the current move (the element marked `aria-current`) in view inside a scrolling list. One read and one write
 * per frame however fast the moves change, a little room kept past the edge, and a short glide for a near move where
 * a far one jumps.
 */
export function followCurrent(list: Ref<HTMLElement | undefined>, source: WatchSource) {
  let frame = 0;
  function settle() {
    frame = 0;
    const box = list.value;
    const item = box?.querySelector<HTMLElement>("[aria-current]");
    if (!box || !item) return;
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
