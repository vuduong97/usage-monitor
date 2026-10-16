// W · 1-bit: charcoal on warm grey, like a Playdate or e-ink; patterns instead of colour.
module.exports = {
  id: 'onebit', name: '1-bit', bg: '#b1afa8', light: true,
  draw(S) {
    const bg = '#b1afa8', ink = '#312f28';
    S.text('{clock}', 120, 2, 'm32', ink, { align: 'center' });
    S.rect(8, 46, 224, 2, ink, { pattern: ['cols', 4, -8, 2] });

    S.text('5H', 10, 58, 'm18', ink);
    S.text('{s}%', 8, 78, 'm32', ink, { when: 'ok' });
    S.text('--%', 8, 78, 'm32', ink, { when: '!ok' });
    S.text('RST {sr@}', 10, 120, 'm18', ink, { when: 'ok' });
    S.text('NO DATA', 10, 120, 'm18', ink, { when: '!ok' });
    const bob = S.track(600, [{ dy: 0 }, { dy: 1 }]);
    S.mask('clawd', 160, 56, 2, ink, { when: 'ok', track: bob });
    S.mask('clawd', 160, 56, 2, '#77746b', { when: '!ok', track: bob });

    // solid = used, dither = time gone: dither showing past the solid means you are under pace
    const bar = (x, y, w, h, src, tsrc) => {
      S.strokeRect(x, y, w, h, ink);
      const iw = w - 4;
      S.rect(x + 2, y + 2, 0, h - 4, ink, { when: 'ok', dx: [src, iw], x1: [tsrc, iw], pattern: ['diag', 2, 0] });
      S.rect(x + 2, y + 2, 0, h - 4, ink, { when: 'ok', x1: [src, iw] });
    };
    bar(8, 146, 224, 18, 's', 'ts');
    S.text('7D {w}%', 10, 170, 'm18', ink, { when: 'ok' });
    S.text('7D --%', 10, 170, 'm18', ink, { when: '!ok' });
    S.text('{wr@d}', 230, 170, 'm18', ink, { align: 'right', when: 'ok', upper: true });
    bar(8, 192, 224, 12, 'w', 'tw');

    // the bottom strip; over pace it flashes inverted every 500 ms
    const flash = S.track(500, [{}, { color: bg }]);
    S.rect(0, 212, 240, 28, ink, { when: '!over' });
    S.rect(0, 212, 240, 28, ink, { when: 'over', track: flash });
    const edge = S.track(500, [{ vis: false }, {}]);
    S.strokeRect(0, 212, 240, 28, ink, { when: 'over', track: edge });
    const textFlash = S.track(500, [{}, { color: ink }]);
    for (const [when, msg] of [['!ok', '* WAITING FOR APP'], ['ok fresh', '* FRESH WINDOW'],
      ['ok !fresh over', '! OVER PACE'], ['ok !fresh !over', '* ON TRACK']]) {
      S.text(msg, 120, 217, 'm18', bg, { when: `${when} !over`, align: 'center' });
      S.text(msg, 120, 217, 'm18', bg, { when: `${when} over`, align: 'center', track: textFlash });
    }
  },
};
