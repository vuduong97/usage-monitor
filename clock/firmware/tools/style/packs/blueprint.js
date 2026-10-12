// U · Blueprint: white line work on a gridded blue sheet; used is hatched, sizes are dimensioned.
module.exports = {
  id: 'blueprint', name: 'Blueprint', bg: '#0d3b7a',
  draw(S) {
    const B = { minor: '#164a8f', major: '#2559a3', line: '#eaf2ff', note: '#9cc7ff', warn: '#ff8a7a' };
    // the sheet's grid: every 10 px, every 40 px brighter
    S.rect(0, 0, 240, 240, B.minor, { pattern: ['cols', 10, 0, 1] });
    S.rect(0, 0, 240, 240, B.minor, { pattern: ['rows', 10, 0, 1] });
    S.rect(0, 0, 240, 240, B.major, { pattern: ['cols', 40, 0, 1] });
    S.rect(0, 0, 240, 240, B.major, { pattern: ['rows', 40, 0, 1] });

    // a dimension: line, an arrow head at each end, the label between them
    const arrows = (when, col, x0, x1, y, label, above, bindLeft) => {
      const o = (extra) => Object.assign({ when }, extra);
      const L = bindLeft ? { dx: ['s', 208] } : {};
      if (bindLeft) S.line(x0, y, x1, y, col, o({ dx: ['s', 208], x1: ['fixed', 0] }));
      else S.line(x0, y, x0, y, col, o({ x1: ['s', 208] }));
      S.tri([[x0, y], [x0 + 5, y - 3], [x0 + 5, y + 3]], col, o(L));
      if (bindLeft) S.tri([[x1, y], [x1 - 5, y - 3], [x1 - 5, y + 3]], col, o({}));
      else S.tri([[x0, y], [x0 - 5, y - 3], [x0 - 5, y + 3]], col, o({ dx: ['s', 208] }));
      const mid = bindLeft ? (x0 + x1) / 2 : x0;
      S.text(label, Math.round(mid), above ? y - 16 : y + 3, 's14', col, o({ align: 'center', clamp: true, dx: ['s', 104] }));
    };
    const hatchBar = (x, y, w, h, src) => {
      S.rect(x, y, 0, h, B.line, { when: 'ok', x1: [src, w], pattern: ['diag', 5, -(x + y)] });
      S.strokeRect(x, y, w, h, B.line);
      S.rect(x, y, 1, h, B.line, { when: 'ok', dx: [src, w], gate: [src, 0.5 / w, 1 - 0.5 / w] });
    };

    S.text('FIG. 1  5-HOUR WINDOW', 8, 4, 's14', B.note);
    hatchBar(16, 46, 208, 22, 's');
    arrows('over', B.warn, 16, 0, 36, '{s}% USED', true, false);
    arrows('ok !over', B.line, 16, 0, 36, '{s}% USED', true, false);
    arrows('ok', B.note, 16, 224, 76, '{sleft}% LEFT', false, true);
    for (let y = 42; y < 72; y += 4) S.rect(16, y, 1, 2, B.warn, { when: 'ok', dx: ['ts', 208] });
    S.text('NO DATA', 120, 50, 's14', B.note, { align: 'center', when: '!ok' });

    S.text('FIG. 2  WEEKLY', 8, 104, 's14', B.note);
    S.text('{w}%', 224, 104, 's14', B.line, { align: 'right', when: 'ok' });
    S.text('--', 224, 104, 's14', B.line, { align: 'right', when: '!ok' });
    hatchBar(16, 124, 208, 14, 'w');

    S.mask('clawd_inner', 16, 156, 2, '#0d3b7a');
    S.mask('clawd_edge', 16, 156, 2, B.line);
    S.line(70, 168, 84, 152, B.note);
    S.line(84, 152, 92, 152, B.note);
    S.text('CLAWD MK.I', 94, 145, 's14', B.note);

    S.strokeRect(90, 166, 147, 67, B.line);
    S.rect(91, 188, 145, 1, B.line);
    S.rect(91, 210, 145, 1, B.line);
    S.text('RESET {sr@}', 96, 170, 's14', B.warn, { when: 'over' });
    S.text('RESET {sr@}', 96, 170, 's14', B.line, { when: 'ok !over' });
    S.text('RESET ---', 96, 170, 's14', B.line, { when: '!ok' });
    S.text('WEEK {wr@d}', 96, 192, 's14', B.line, { when: 'ok', upper: true });
    S.text('WEEK ---', 96, 192, 's14', B.line, { when: '!ok' });
    S.text('DRAWN {clock}  1:1', 96, 214, 's14', B.note);
  },
};
