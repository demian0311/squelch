// One guide per rail entry: `ui/src/guides/<id>.md`, a page that says what the category is for, what you hear, how the
// radio reads it, the band plan and the rules. Markdown, a small dialect rendered here (no dependency, and the output is
// known): `#` title, `##` sections, paragraphs, `-` lists, `|` tables, **bold**, *italic*, `code`, [links](url) and
// `![caption](diagram/name)` for an SVG from `ui/src/guides/diagrams/`. A link to `tune:27.185` or `tune:27.385/lsb`
// tunes, `band:iss` opens that rail entry, `https://` opens the browser; the screen handles the clicks.

import { EMS_IDS } from './ems';

const pages = import.meta.glob<string>('../guides/*.md', { query: '?raw', import: 'default', eager: true });
const diagrams = import.meta.glob<string>('../guides/diagrams/*.svg', { query: '?raw', import: 'default', eager: true });

const nameOf = (path: string) => path.slice(path.lastIndexOf('/') + 1).replace(/\.(md|svg)$/, '');
export const GUIDES: Record<string, string> = Object.fromEntries(Object.entries(pages).map(([p, s]) => [nameOf(p), s]));
export const DIAGRAMS: Record<string, string> = Object.fromEntries(Object.entries(diagrams).map(([p, s]) => [nameOf(p), s]));

/** The ham bands below the tuner, one guide between them. */
export const HF_HAM_IDS = ['80m', '40m', '20m'];

/** The guide a band belongs to: the four EMS bands share one, the aircraft bands share the Airplanes one, the HF ham bands theirs. */
export function guideFor(bandId: string | null | undefined): string | null {
  if (!bandId) return null;
  if (EMS_IDS.includes(bandId)) return 'ems';
  if (bandId === 'adsb' || bandId === 'acars') return 'air';
  if (HF_HAM_IDS.includes(bandId)) return 'hf';
  return bandId in GUIDES ? bandId : null;
}

export interface Guide {
  /** The `#` line. */
  title: string;
  html: string;
}

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** Bold, italic, code and links inside a line. */
export function inline(s: string): string {
  let out = '';
  let i = 0;
  // code first, so nothing inside it is read as markup
  const parts = s.split(/(`[^`]+`)/);
  for (const part of parts) {
    if (part.startsWith('`') && part.endsWith('`') && part.length > 1) {
      out += `<code>${esc(part.slice(1, -1))}</code>`;
      continue;
    }
    let t = esc(part);
    t = t.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, text, href) => {
      const kind = href.startsWith('tune:') ? 'tune' : href.startsWith('band:') ? 'band' : 'out';
      return `<a href="${href}" data-act="${kind}"${kind === 'out' ? ' title="Opens in the browser"' : ''}>${text}</a>`;
    });
    t = t.replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>');
    t = t.replace(/\*([^*]+)\*/g, '<i>$1</i>');
    out += t;
    i++;
  }
  return out;
}

const cells = (line: string) =>
  line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((c) => c.trim());

/** Renders a guide. Unknown diagram names throw, so a test catches a typo before a user sees a hole. */
export function renderGuide(md: string): Guide {
  const lines = md.replace(/\r/g, '').split('\n');
  let title = '';
  const out: string[] = [];
  let open = false; // inside a <section>
  let i = 0;
  const closeSection = () => {
    if (open) out.push('</section>');
    open = false;
  };
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i++;
      continue;
    }
    if (line.startsWith('# ')) {
      title = line.slice(2).trim();
      i++;
      continue;
    }
    if (line.startsWith('## ')) {
      closeSection();
      out.push(`<section><h2>${inline(line.slice(3).trim())}</h2>`);
      open = true;
      i++;
      continue;
    }
    if (!open) {
      out.push('<section class="lead">');
      open = true;
    }
    const fig = /^!\[([^\]]*)\]\(diagram\/([\w-]+)\)\s*$/.exec(line);
    if (fig) {
      const svg = DIAGRAMS[fig[2]];
      if (!svg) throw new Error(`guide: no diagram "${fig[2]}"`);
      out.push(`<figure>${svg}${fig[1] ? `<figcaption>${inline(fig[1])}</figcaption>` : ''}</figure>`);
      i++;
      continue;
    }
    if (line.startsWith('- ')) {
      const items: string[] = [];
      while (i < lines.length && lines[i].startsWith('- ')) items.push(`<li>${inline(lines[i].slice(2).trim())}</li>`), i++;
      out.push(`<ul>${items.join('')}</ul>`);
      continue;
    }
    if (line.trim().startsWith('|')) {
      const rows: string[][] = [];
      while (i < lines.length && lines[i].trim().startsWith('|')) {
        const c = cells(lines[i]);
        if (!c.every((x) => /^:?-+:?$/.test(x))) rows.push(c);
        i++;
      }
      const [head, ...body] = rows;
      const tr = (r: string[], tag: string) => `<tr>${r.map((c) => `<${tag}>${inline(c)}</${tag}>`).join('')}</tr>`;
      out.push(`<table><thead>${tr(head, 'th')}</thead><tbody>${body.map((r) => tr(r, 'td')).join('')}</tbody></table>`);
      continue;
    }
    const para: string[] = [];
    while (i < lines.length && lines[i].trim() && !/^(#|- |\||!\[)/.test(lines[i])) para.push(lines[i].trim()), i++;
    out.push(`<p>${inline(para.join(' '))}</p>`);
  }
  closeSection();
  return { title, html: out.join('') };
}

export function guide(id: string): Guide | null {
  const md = GUIDES[id];
  return md ? renderGuide(md) : null;
}
