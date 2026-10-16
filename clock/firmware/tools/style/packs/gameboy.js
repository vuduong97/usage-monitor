// R · Game Boy: the original Game Boy's four greens and nothing else.
module.exports = {
  id: 'gameboy', name: 'Game Boy', bg: '#9bbc0f',
  draw(S) {
    const G = ['#0f380f', '#306230', '#8bac0f', '#9bbc0f'];
    S.rect(0, 0, 240, 26, G[0]);
    S.text('{clock}', 8, 4, 'm18', G[3]);
    S.text('USAGE', 232, 4, 'm18', G[2], { align: 'right' });

    const bob = S.track(500, [{ dy: 0 }, { dy: 2 }]);
    S.mask('clawd', 12, 36, 2, G[1], { track: bob });
    S.mask('clawd_eyes', 12, 36, 2, G[0], { track: bob });
    S.rect(14, 118, 60, 4, G[2], { r: 2 });

    S.text('5-HOUR', 92, 36, 'm18', G[1]);
    S.text('{s}%', 90, 56, 'm32', G[0], { when: 'ok !hot' });
    S.text('{s}%', 90, 56, 'm32', G[0], { when: 'hot', track: S.track(400, [{}, { color: G[2] }]) });
    S.text('--%', 90, 56, 'm32', G[0], { when: '!ok' });

    const pips = (x, y, n, pw, ph, gap, src) => {
      for (let i = 0; i < n; i++) {
        S.rect(x + i * (pw + gap), y, pw, ph, G[2]);
        S.rect(x + i * (pw + gap), y, pw, ph, G[0], { when: 'ok', gate: [src, (i + 0.5) / n] });
      }
    };
    pips(92, 96, 10, 12, 12, 2, 's');
    S.text('WEEKLY', 12, 128, 'm18', G[1]);
    S.text('{w}%', 228, 128, 'm18', G[0], { align: 'right', when: 'ok' });
    S.text('--%', 228, 128, 'm18', G[0], { align: 'right', when: '!ok' });
    pips(12, 150, 20, 9, 10, 2, 'w');

    S.rect(6, 170, 228, 64, G[0], { r: 4 });
    S.rect(9, 173, 222, 58, G[3], { r: 2 });
    S.text('RESET {sr@}', 18, 180, 'm18', G[0], { when: 'ok', upper: true });
    S.text('WEEK  {wr@d}', 18, 204, 'm18', G[0], { when: 'ok', upper: true });
    S.text('NO SIGNAL...', 18, 180, 'm18', G[0], { when: '!ok' });
    S.text('IS THE APP ON?', 18, 204, 'm18', G[0], { when: '!ok' });
    S.tri([[214, 204], [214, 218], [222, 211]], G[0], { track: S.blink(450) });
  },
};
