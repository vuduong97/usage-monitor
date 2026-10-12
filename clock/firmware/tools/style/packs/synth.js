// S · Synthwave: a striped sun setting on a perspective grid that rolls toward you.
module.exports = {
  id: 'synth', name: 'Synthwave', bg: '#12002b',
  draw(S, { mix }) {
    const sky = ['#12002b', '#1a0436', '#230842', '#2e0c4f', '#3b105c', '#4a1466'];
    sky.forEach((c, i) => S.rect(0, i * 28, 240, i === 5 ? 32 : 28, c));
    const cyan = '#00e5ff', pink = '#ff2fd0';

    // the clock with a cheap glow: a dim copy one pixel out on each side
    const glow = mix(cyan, '#1a0436', 0.55);
    for (const [dx, dy] of [[-1, 0], [1, 0], [0, -1], [0, 1]]) S.text('{clock}', 120 + dx, 4 + dy, 's48', glow, { align: 'center' });
    S.text('{clock}', 120, 4, 's48', '#b8f6ff', { align: 'center' });

    const bar = (y, label, src) => {
      S.text(label, 16, y, 's14', '#ff9ae6');
      S.strokeRect(44, y + 3, 149, 10, cyan);
      if (src === 's') {
        S.rect(46, y + 5, 0, 6, pink, { when: 'ok !hot', x1: ['s', 145] });
        S.rect(46, y + 5, 0, 6, '#ff3b3b', { when: 'hot', x1: ['s', 145] });
      } else {
        S.rect(46, y + 5, 0, 6, pink, { when: 'ok', x1: ['w', 145] });
      }
      S.text(`{${src}}%`, 228, y, 's14', '#e9e0ff', { align: 'right', when: 'ok' });
      S.text('--', 228, y, 's14', '#e9e0ff', { align: 'right', when: '!ok' });
    };
    bar(60, '5H', 's');
    bar(80, '7D', 'w');
    S.text('reset {sr@}  \u00b7  week {wr@d}', 120, 100, 's14', '#a98bdf', { align: 'center', when: 'ok' });
    S.text('no signal', 120, 100, 's14', '#a98bdf', { align: 'center', when: '!ok' });

    // the sun: a gradient half disc, rows worked out here; five sky stripes across it
    const hz = 172, sr = 50, cx = 120;
    const half = (y) => Math.sqrt(Math.max(0, sr * sr - (hz - (y + 0.5)) ** 2));
    for (const [when, sun] of [['!hot', ['#ffd23f', '#ff8c42', '#ff2fd0']], ['hot', ['#ff6a3d', '#ff3b3b', '#d61f5a']]]) {
      for (let y = hz - sr; y < hz; y++) {
        const k = (y - (hz - sr)) / sr;
        const c = k < 0.5 ? mix(sun[0], sun[1], k * 2) : mix(sun[1], sun[2], (k - 0.5) * 2);
        const h = half(y), x0 = Math.round(cx - h), x1 = Math.round(cx + h);
        S.rect(x0, y, x1 - x0, 1, c, { when });
      }
    }
    for (let i = 0; i < 5; i++) {
      const y0 = hz - 22 + i * 5, n = Math.round(1 + i * 0.6);
      for (let y = y0; y < y0 + n && y < hz; y++) {
        const h = half(y), x0 = Math.round(cx - h), x1 = Math.round(cx + h);
        S.rect(x0, y, x1 - x0, 1, '#3b105c');
      }
    }

    // the grid: eight lines that slide toward you (square law), fading in, every 1.4 s
    S.rect(0, hz, 240, 240 - hz, '#12002b');
    const STEPS = 28;
    const lineY = (i, p) => Math.round(hz + 2 + (((i + p / STEPS) / 8) ** 2) * (240 - hz));
    for (let i = 0; i < 8; i++) {
      const y0 = lineY(i, 0);
      const steps = [];
      for (let p = 0; p < STEPS; p++) {
        const k = ((i + p / STEPS) / 8) ** 2;
        steps.push({ dy: lineY(i, p) - y0, color: mix('#12002b', pink, 0.35 + 0.65 * k) });
      }
      S.rect(0, y0, 240, 1, mix('#12002b', pink, 0.35 + 0.65 * (i / 8) ** 2), { track: S.track(1400 / STEPS, steps) });
    }
    for (let i = -6; i <= 6; i++) S.line(cx + i * 8, hz, cx + i * 70, 240, pink);
    S.rect(0, hz, 240, 1, '#ff9ae6');

    // Clawd in front of the sun: a neon silhouette with a soft halo, bobbing while data flows
    const bob = S.track(350, [{ dy: 0 }, { dy: -1 }]);
    const halo = mix(cyan, '#3b105c', 0.6);
    for (const [dx, dy] of [[-1, 0], [1, 0], [0, -1], [0, 1]]) S.mask('clawd', 104 + dx, hz - 38 + dy, 1, halo, { when: 'ok', track: bob });
    S.mask('clawd', 104, hz - 38, 1, cyan, { when: 'ok', track: bob });
    S.mask('clawd_eyes', 104, hz - 38, 1, '#12002b', { when: 'ok', track: bob });
    S.mask('clawd', 104, hz - 38, 1, '#3a6d78', { when: '!ok' });
    S.mask('clawd_eyes', 104, hz - 38, 1, '#12002b', { when: '!ok' });
  },
};
