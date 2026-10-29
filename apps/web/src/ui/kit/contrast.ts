/** A colour as 0 to 255 red, green and blue. */
type Rgb = [number, number, number];

let canvas: CanvasRenderingContext2D | null | undefined;

/**
 * Any CSS colour the page understands (a `var()`, a `color-mix()`) as plain sRGB, read back from the pixel a canvas
 * paints with it. `where` is the element whose custom properties apply, so the answer follows the scheme on screen.
 */
export function resolveColor(css: string, where: HTMLElement = document.documentElement): Rgb | null {
  const probe = document.createElement("span");
  probe.style.color = css;
  where.append(probe);
  const computed = getComputedStyle(probe).color;
  probe.remove();
  canvas ??= document.createElement("canvas").getContext("2d", { willReadFrequently: true });
  if (!canvas) return null;
  canvas.clearRect(0, 0, 1, 1);
  canvas.fillStyle = "#000";
  canvas.fillStyle = computed;
  canvas.fillRect(0, 0, 1, 1);
  const [r, g, b] = canvas.getImageData(0, 0, 1, 1).data;
  return r === undefined || g === undefined || b === undefined ? null : [r, g, b];
}

const channel = (v: number) => {
  const s = v / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
};

/** WCAG relative luminance. */
const luminance = ([r, g, b]: Rgb) => 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);

/** WCAG contrast ratio of two colours, 1 to 21. */
export function contrastRatio(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}
