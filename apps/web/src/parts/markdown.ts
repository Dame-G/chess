/**
 * Notes are markdown, rendered simply: paragraphs, bullet and numbered lists, `##` headings, **bold**, *italics* and
 * `code`. Nothing is passed to the page as HTML; the components draw the tokens, so what an idea's author wrote can
 * never inject markup.
 */
export interface Span {
  kind: "text" | "bold" | "italic" | "code";
  text: string;
}

export type Block =
  { type: "p"; spans: Span[] } | { type: "h"; spans: Span[] } | { type: "ul" | "ol"; items: Span[][] };

const INLINE = /(\*\*[^*]+\*\*|`[^`]+`|\*[^*\s][^*]*\*)/g;

export function inline(text: string): Span[] {
  const spans: Span[] = [];
  let last = 0;
  for (const m of text.matchAll(INLINE)) {
    if (m.index > last) spans.push({ kind: "text", text: text.slice(last, m.index) });
    const t = m[0];
    if (t.startsWith("**")) spans.push({ kind: "bold", text: t.slice(2, -2) });
    else if (t.startsWith("`")) spans.push({ kind: "code", text: t.slice(1, -1) });
    else spans.push({ kind: "italic", text: t.slice(1, -1) });
    last = m.index + t.length;
  }
  if (last < text.length) spans.push({ kind: "text", text: text.slice(last) });
  return spans;
}

/** Blocks of a markdown text, separated by blank lines; a list runs while its lines start with a marker. */
export function blocks(markdown: string): Block[] {
  const out: Block[] = [];
  let paragraph: string[] = [];
  const flush = () => {
    if (paragraph.length) out.push({ type: "p", spans: inline(paragraph.join(" ")) });
    paragraph = [];
  };
  for (const raw of markdown.replace(/\r\n/g, "\n").split("\n")) {
    const line = raw.trim();
    const bullet = /^[-*]\s+(.*)$/.exec(line);
    const number = /^\d+[.)]\s+(.*)$/.exec(line);
    const heading = /^#{1,6}\s+(.*)$/.exec(line);
    if (!line) {
      flush();
    } else if (heading) {
      flush();
      out.push({ type: "h", spans: inline(heading[1]!) });
    } else if (bullet || number) {
      flush();
      const type = bullet ? "ul" : "ol";
      const item = inline((bullet ?? number)![1]!);
      const last = out.at(-1);
      if (last && last.type === type) last.items.push(item);
      else out.push({ type, items: [item] });
    } else {
      paragraph.push(line);
    }
  }
  flush();
  return out;
}
