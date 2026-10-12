// T · Newspaper: cream paper, ink, a big Tiempos number and the forecast as a sentence.
module.exports = {
  id: 'paper', name: 'Newspaper', bg: '#f4efe6', light: true,
  draw(S) {
    const P = { ink: '#1f1e1b', mute: '#6b655a', rule: '#d3cabb', track: '#e4dccd', acc: '#b5532f', red: '#b3261e' };
    S.text('Usage', 10, 6, 't34', P.ink);
    S.text('{dow} {clock}', 230, 20, 's14', P.mute, { align: 'right' });
    S.rect(10, 46, 220, 2, P.ink);
    S.rect(10, 50, 220, 1, P.ink);

    S.text('5-HOUR WINDOW', 10, 58, 's14', P.acc);
    S.text('{s}%', 8, 72, 't56', P.ink, { when: 'ok !hot' });
    S.text('{s}%', 8, 72, 't56', P.red, { when: 'hot' });
    S.text('--', 8, 72, 't56', P.mute, { when: '!ok' });
    S.rect(10, 134, 132, 4, P.track, { r: 2 });
    S.rect(10, 134, 0, 4, P.ink, { r: 2, when: 'ok !hot', x1: ['s', 132], minW: 2, zeroOk: true });
    S.rect(10, 134, 0, 4, P.red, { r: 2, when: 'hot', x1: ['s', 132], minW: 2, zeroOk: true });

    S.rect(152, 58, 1, 84, P.rule);
    S.mask('clawd', 164, 60, 2, '#d97757', { when: 'ok' });
    S.mask('clawd', 164, 60, 2, '#c9bfae', { when: '!ok' });
    S.rect(10, 150, 220, 1, P.rule);

    const say = (when, a, b, col) => { S.text(a, 10, 158, 's14', col, { when }); S.text(b, 10, 176, 's14', col, { when }); };
    say('!ok', 'No dispatch from the app yet.', 'Is the desktop app running?', P.mute);
    say('ok fresh', 'A fresh window just opened.', 'It resets again at {sr@}.', P.ink);
    say('ok !fresh out', 'At this pace: out at {out},', 'before the {sr@} reset.', P.red);
    say('ok !fresh !out', 'At this pace it lasts until the', 'reset at {sr@}, near {end}%.', P.ink);

    S.rect(10, 200, 220, 1, P.rule);
    S.text('Weekly', 10, 207, 's14', P.acc);
    S.text('{w}%', 62, 207, 's14', P.ink, { when: 'ok' });
    S.text('--', 62, 207, 's14', P.ink, { when: '!ok' });
    S.text('resets {wr@d}', 230, 207, 's14', P.mute, { align: 'right', when: 'ok' });
    S.rect(10, 228, 220, 3, P.track, { r: 1 });
    S.rect(10, 228, 0, 3, P.ink, { r: 1, when: 'ok !whot', x1: ['w', 220], minW: 2, zeroOk: true });
    S.rect(10, 228, 0, 3, P.red, { r: 1, when: 'whot', x1: ['w', 220], minW: 2, zeroOk: true });
  },
};
