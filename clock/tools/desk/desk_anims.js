// desk_anims.js — which clawd-on-desk SVGs go into /desk.pak, and under which
// name the clock (and the daemon) refer to them. Names are <= 15 chars.
'use strict';

module.exports = {
  GRID: 60,              // output grid (60x60 cells, 4 px each on the 240 px panel)
  CPU: 2,                // grid cells per SVG unit
  WINDOW: { x: -7.5, y: -12.5 }, // SVG-unit origin of the 30x30-unit window (from --probe: Clawd centred, feet + shadow kept)
  FPS: 10,
  MAX_MS: 6000,          // longest clip rendered per animation
  MAX_CELLS: 3328,       // crop limit: the clock decodes a frame into a 1,664 B buffer
  ANIMS: [
    // agent states
    { name: 'thinking',     svg: 'clawd-working-thinking.svg' },
    { name: 'typing',       svg: 'clawd-working-typing.svg' },
    { name: 'building',     svg: 'clawd-working-building.svg' },
    { name: 'headphones',   svg: 'clawd-headphones-groove.svg' },
    { name: 'juggling',     svg: 'clawd-working-juggling.svg' },
    { name: 'error',        svg: 'clawd-error.svg' },
    { name: 'happy',        svg: 'clawd-happy.svg' },
    { name: 'notification', svg: 'clawd-notification.svg' },
    { name: 'sweeping',     svg: 'clawd-working-sweeping.svg' },
    { name: 'carrying',     svg: 'clawd-working-carrying.svg' },
    // pet
    { name: 'idle',         svg: 'clawd-idle-living.svg' },
    { name: 'reading',      svg: 'clawd-idle-reading.svg' },
    { name: 'yawn',         svg: 'clawd-idle-yawn.svg' },
    { name: 'doze',         svg: 'clawd-idle-doze.svg' },
    { name: 'sleeping',     svg: 'clawd-sleeping.svg' },
    { name: 'wake',         svg: 'clawd-wake.svg' },
    { name: 'low battery',  svg: 'clawd-idle-low-battery.svg' },
    { name: 'double jump',  svg: 'clawd-react-double-jump.svg' },
    { name: 'heart eyes',   svg: 'clawd-heart-eyes.svg' },
  ],
};
