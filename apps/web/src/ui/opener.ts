/**
 * What opened the sheet on screen (a header chip, a link, a button): focus goes back there when the last sheet closes.
 * Moving from one sheet to the next keeps the first opener, so closing still lands where it all began.
 */
let carried: HTMLElement | null = null;

/** Called as a sheet is set up: takes the focused element as the opener unless it sits in a sheet already. */
export function captureOpener(): HTMLElement | null {
  const here = document.activeElement;
  if (here instanceof HTMLElement && here !== document.body && !here.closest(".ui-sheet")) carried = here;
  return carried;
}

/**
 * Called as a sheet goes: when focus was inside it (or lost), puts focus back on the opener once the sheet is gone,
 * unless another sheet took its place or focus has gone somewhere on purpose.
 */
export function returnToOpener(
  sheet: HTMLElement | null,
  opener: HTMLElement | null,
  after: (run: () => void) => void,
) {
  const here = document.activeElement;
  if (here && here !== document.body && !sheet?.contains(here)) return;
  after(() => {
    if (document.querySelector(".ui-sheet")) return;
    const lost = !document.activeElement || document.activeElement === document.body;
    if (lost && opener?.isConnected) opener.focus({ preventScroll: true });
    carried = null;
  });
}
