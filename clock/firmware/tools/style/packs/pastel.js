// V · Pastel: milk-pink sheet, jelly bars, a blushing Clawd with a floating heart and sparkles.
module.exports = {
  id: 'pastel', name: 'Pastel', bg: '#fff4f8', light: true,
  draw(S, { mix }) {
    const K = { bg: '#fff4f8', ink: '#5b4b6b', mute: '#a596b5', pink: '#ff8fab', pinkT: '#ffe1ea', lav: '#a98bff', lavT: '#ece6ff', mint: '#7fd6b3', butter: '#ffd36e', coral: '#ff6b6b' };
    S.text('{clock}', 12, 4, 't34', K.ink);
    for (const [when, label, col] of [['!ok', 'no data', K.mute], ['hot', 'running hot', K.coral],
      ['ok !hot paceover', 'a bit fast', K.coral], ['ok !hot !paceover', 'all good', K.mint]]) {
      const bw = S.width('s14', label) + 22;
      S.rect(228 - bw, 12, bw, 22, col, { r: 11, when });
      S.text(label, 228 - bw + 11, 15, 's14', '#ffffff', { when });
    }

    // Clawd breathes up and down (2 px, 2.88 s); the cheeks and the sweat drop ride along
    const PERIOD = 2880, bobAt = (t) => Math.round(2 * Math.sin((2 * Math.PI * t) / PERIOD));
    const bob = S.track(120, Array.from({ length: PERIOD / 120 }, (_, k) => ({ dy: bobAt(k * 120) })));
    S.mask('clawd', 14, 50, 2, '#f08a6a', { track: bob });
    S.rect(18, 84, 10, 5, '#ffb3c7', { r: 3, track: bob });
    S.rect(64, 84, 10, 5, '#ffb3c7', { r: 3, track: bob });
    // sweat: a drop that slides 12 px every 480 ms beside Clawd while usage runs hot
    S.defMask('sweat', 10, 14, (px, py) => {
      const X = px + 0.5 - 5, Y = py + 0.5 - 7;            // relative to the drop's (x, y)
      return X * X + (Y - 2) ** 2 <= 16 || (Y >= -7 && Y <= 1 && Math.abs(X) <= (4 * (Y + 7)) / 8);
    });
    const sweat = S.track(40, Array.from({ length: PERIOD / 40 }, (_, k) => ({ dy: bobAt(k * 40) + (k % 12) })));
    S.mask('sweat', 84 - 5, 50 + 14 - 7, 1, '#6a9bcc', { when: 'hot', track: sweat });

    // a heart floats up from Clawd and fades out, every 2.2 s
    S.defMask('heart', 12, 12, (px, py) => {
      const X = px + 0.5 - 6, Y = py + 0.5 - 3;             // heart centre (x, y) = (6, 3)
      const lobe = (cx) => (X - cx) ** 2 + Y * Y <= 2.5 * 2.5;
      const tri = Y >= 1 && Y <= 8 && Math.abs(X) <= (5 * (8 - Y)) / 7;
      return lobe(-2.5) || lobe(2.5) || tri;
    });
    const heart = S.track(100, Array.from({ length: 22 }, (_, k) => ({ dy: Math.round(-(k / 22) * 16), color: mix(K.bg, K.pink, 1 - k / 22) })));
    S.mask('heart', 80 - 6, 60 - 3, 1, K.pink, { when: 'ok', track: heart });

    const twinkle = S.track(350, [{ vis: false }, {}, {}]);
    [[214, 56, 0], [196, 120, 1], [100, 128, 2]].forEach(([x, y, o]) => {
      S.rect(x - 1, y - 5, 2, 10, K.butter, { r: 1, track: twinkle, phase: o * 350 });
      S.rect(x - 5, y - 1, 10, 2, K.butter, { r: 1, track: twinkle, phase: o * 350 });
    });

    S.text('5-hour', 96, 58, 's14', K.mute);
    S.text('{s}%', 94, 74, 's48', K.ink, { when: 'ok !hot' });
    S.text('{s}%', 94, 74, 's48', K.coral, { when: 'hot' });
    S.text('--%', 94, 74, 's48', K.ink, { when: '!ok' });
    S.text('back at {sr@}', 12, 146, 's14', K.mute, { when: 'ok' });
    S.text('waiting...', 12, 146, 's14', K.mute, { when: '!ok' });
    S.text('{sr~}', 228, 146, 's14', K.mute, { align: 'right', when: 'ok' });

    const pill = (y, src, hotWhen, coolWhen, fill, track) => {
      S.rect(12, y, 216, 22, track, { r: 11 });
      for (const [when, col] of [[hotWhen, K.coral], [coolWhen, fill]]) {
        S.rect(12, y, 0, 22, col, { r: 11, when, x1: [src, 216], minW: 22, gate: [src, 0.0001] });
        S.rect(20, y + 5, -28, 4, mix(col, '#ffffff', 0.45), { r: 2, when, x1: [src, 216] });
      }
    };
    pill(164, 's', 'hot', 'ok !hot', K.pink, K.pinkT);
    S.text('weekly', 12, 192, 's14', K.mute);
    S.text('{w}% \u00b7 {wr~}', 228, 192, 's14', K.ink, { align: 'right', when: 'ok' });
    S.text('--', 228, 192, 's14', K.ink, { align: 'right', when: '!ok' });
    pill(210, 'w', 'whot', 'ok !whot', K.lav, K.lavT);
  },
};
