// desk_anims.js — which clawd-on-desk SVGs go into /desk.pak, and under which
// name the clock (and the daemon) refer to them. Names are <= 15 chars.
//
// Every animation is rendered 1:1 on the 240x240 panel. Per animation:
//   fps   frames per second (default FPS); slow loops can use less to save space
//   maxMs cap on the rendered clip (default MAX_MS)
//   smil  also step the SVG's own <animate> timeline (an SVG animated only that way)
'use strict';

module.exports = {
  SIZE: 240,             // panel pixels (square)
  PX_PER_UNIT: 8,        // panel pixels per SVG unit
  WINDOW: { x: -7.5, y: -12.5 }, // SVG-unit origin of the 30x30-unit window (Clawd centred, feet + shadow kept)
  FPS: 20,
  MAX_MS: 16000,         // longest clip rendered per animation
  ANIMS: [
    // agent states
    { name: 'thinking',     svg: 'clawd-working-thinking.svg', fps: 15 },
    { name: 'ultrathink',   svg: 'clawd-working-ultrathink.svg' },
    { name: 'typing',       svg: 'clawd-working-typing.svg' },
    { name: 'typing boss',  svg: 'clawd-working-typing-boss.svg' },
    { name: 'building',     svg: 'clawd-working-building.svg', fps: 15 },
    { name: 'wizard',       svg: 'clawd-working-wizard.svg', fps: 15 },
    { name: 'debugger',     svg: 'clawd-working-debugger.svg', fps: 15 },
    { name: 'headphones',   svg: 'clawd-headphones-groove.svg' },
    { name: 'juggling',     svg: 'clawd-working-juggling.svg' },
    { name: 'error',        svg: 'clawd-error.svg' },
    { name: 'dizzy',        svg: 'clawd-dizzy.svg', fps: 15 },
    { name: 'happy',        svg: 'clawd-happy.svg' },
    { name: 'notification', svg: 'clawd-notification.svg' },
    { name: 'sweeping',     svg: 'clawd-working-sweeping.svg', fps: 15 },
    { name: 'carrying',     svg: 'clawd-working-carrying.svg' },
    { name: 'reading',      svg: 'clawd-idle-reading.svg' },
    // pet: what the clock slips in between its usage screens when no agent runs
    // (one pass each; yawn -> sleeping -> wake counts as one pick)
    { name: 'idle',         svg: 'clawd-idle-living.svg', fps: 15 },
    { name: 'look',         svg: 'clawd-idle-look.svg', fps: 15 },
    { name: 'bubble',       svg: 'clawd-idle-bubble.svg' },
    { name: 'coffee hand',  svg: 'clawd-coffee-hand.svg', fps: 12, smil: true },
    { name: 'shy',          svg: 'clawd-aegyo-shy.svg', fps: 15 },
    { name: 'outlaw',       svg: 'clawd-outlaw-bender.svg', fps: 8 },
    { name: 'yawn',         svg: 'clawd-idle-yawn.svg' },
    { name: 'sleeping',     svg: 'clawd-sleeping.svg', fps: 15 },
    { name: 'wake',         svg: 'clawd-wake.svg' },
    // pet: moments + pokes
    { name: 'low battery',  svg: 'clawd-idle-low-battery.svg', fps: 15 },
    { name: 'double jump',  svg: 'clawd-react-double-jump.svg' },
    { name: 'heart eyes',   svg: 'clawd-heart-eyes.svg' },
    { name: 'react left',   svg: 'clawd-react-left.svg' },
    { name: 'react right',  svg: 'clawd-react-right.svg' },
    { name: 'annoyed',      svg: 'clawd-react-annoyed.svg' },
  ],
};
