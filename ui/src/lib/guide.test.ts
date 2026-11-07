import { describe, expect, it } from 'vitest';
import { DIAGRAMS, GUIDES, guideFor, inline, renderGuide } from './guide';
import { EMS_IDS } from './ems';

describe('inline', () => {
  it('marks bold, italic, code and escapes the rest', () => {
    expect(inline('a **b** *c* `<d>` <e>')).toBe('a <b>b</b> <i>c</i> <code>&lt;d&gt;</code> &lt;e&gt;');
  });
  it('tells a tune link from a band link from a web link', () => {
    expect(inline('[x](tune:27.185)')).toBe('<a href="tune:27.185" data-act="tune">x</a>');
    expect(inline('[x](band:iss)')).toBe('<a href="band:iss" data-act="band">x</a>');
    expect(inline('[x](https://a.b/c)')).toBe('<a href="https://a.b/c" data-act="out" title="Opens in the browser">x</a>');
  });
});

describe('renderGuide', () => {
  it('takes the title out and wraps sections', () => {
    const g = renderGuide('# CB radio\n\nLead line.\n\n## Rules\n\n- one\n- two\n\n| f | what |\n|---|---|\n| 27.065 | Ch 9 |\n');
    expect(g.title).toBe('CB radio');
    expect(g.html).toBe(
      '<section class="lead"><p>Lead line.</p></section><section><h2>Rules</h2><ul><li>one</li><li>two</li></ul><table><thead><tr><th>f</th><th>what</th></tr></thead><tbody><tr><td>27.065</td><td>Ch 9</td></tr></tbody></table></section>',
    );
  });
  it('joins a wrapped paragraph and stops it at a list', () => {
    expect(renderGuide('## A\nx\ny\n- z').html).toBe('<section><h2>A</h2><p>x y</p><ul><li>z</li></ul></section>');
  });
  it('inlines a diagram and refuses an unknown one', () => {
    const name = Object.keys(DIAGRAMS)[0];
    expect(renderGuide(`## A\n![cap](diagram/${name})`).html).toContain(`<figure><svg`);
    expect(() => renderGuide('## A\n![cap](diagram/no-such)')).toThrow(/no-such/);
  });
});

describe('the guides', () => {
  const rail = ['fm', 'am', 'sw', 'air', 'mil', 'wx', 'marine', 'cb', 'walkie', 'murs', 'rail', 'ems', 'hf', '10m', '6m', '2m', '125', '70cm', '33cm', '23cm', 'sonde', 'ism315', 'ism433', 'ism915', 'iss'];
  it('has one for every rail entry', () => {
    expect(Object.keys(GUIDES).sort()).toEqual([...rail].sort());
  });
  it('every one renders, with a title, a lead and a rules section', () => {
    for (const id of rail) {
      const g = renderGuide(GUIDES[id]);
      expect(g.title, id).not.toBe('');
      expect(g.html, id).toMatch(/^<section class="lead">/);
      expect(g.html, id).toContain('<h2>Rules</h2>');
    }
  });
  it('every diagram is used and every file is an svg', () => {
    const all = Object.values(GUIDES).join('\n');
    for (const [name, svg] of Object.entries(DIAGRAMS)) {
      expect(all, name).toContain(`(diagram/${name})`);
      expect(svg.trimStart().startsWith('<svg'), name).toBe(true);
    }
  });
  it('maps the bands onto guides', () => {
    for (const id of EMS_IDS) expect(guideFor(id)).toBe('ems');
    expect(guideFor('adsb')).toBe('air');
    expect(guideFor('acars')).toBe('air');
    expect(guideFor('cb')).toBe('cb');
    expect(guideFor('nope')).toBeNull();
    expect(guideFor(null)).toBeNull();
  });
});
