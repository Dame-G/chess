import { nextTick, watch, type Ref, type WatchSource } from "vue";

/** Keeps the current move (the element marked `aria-current`) in view inside a scrolling move list. */
export function followCurrent(list: Ref<HTMLElement | undefined>, source: WatchSource) {
  watch(source, async () => {
    await nextTick();
    const box = list.value;
    const item = box?.querySelector<HTMLElement>("[aria-current]");
    if (!box || !item) return;
    const top = item.offsetParent === box ? item.offsetTop : item.offsetTop - box.offsetTop;
    if (top < box.scrollTop) box.scrollTop = top;
    else if (top + item.offsetHeight > box.scrollTop + box.clientHeight)
      box.scrollTop = top + item.offsetHeight - box.clientHeight;
  });
}
