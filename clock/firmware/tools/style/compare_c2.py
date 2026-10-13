#!/usr/bin/env python3
"""compare_c2.py - compare.py for Batch C boards #26-#30 (invaders, eggpet, platformer, chessclock, racinghud).

    python3 tools/style/compare_c2.py <sheet.png> <preview_dir> <out_dir> [id ...]

<sheet.png> is a full-page screenshot of "Clawd style packs - Batch C.html" at 1x (Chrome headless,
--window-size=2200,12500). Each board's 1x strip of the four states sits at (88 + 264 k, top + 651);
the boards are 980 px tall with a 30 px caption and 48 px gap, the first (legend) board 380 px.
Writes <out_dir>/<id>-<k>.png (mock | host | difference, 3x) and prints the share of pixels that differ
by more than 12%.
"""
import os
import subprocess
import sys

STATES = ['normal', 'hot', 'fresh', 'stale']
BOARD = {'invaders': 5, 'eggpet': 6, 'platformer': 7, 'chessclock': 8, 'racinghud': 9}   # after the legend and #21-#25


def top_of(idx):
    return 48 + 30 + 380 + 48 + idx * (30 + 980 + 48) + 30


def main():
    if len(sys.argv) < 4:
        print(__doc__)
        sys.exit(1)
    sheet, prev, out = sys.argv[1:4]
    want = set(sys.argv[4:]) or set(BOARD)
    os.makedirs(out, exist_ok=True)
    for pid, idx in BOARD.items():
        if pid not in want:
            continue
        for k, st in enumerate(STATES):
            host = os.path.join(prev, f'30-{pid}-{k}-{st}.png')
            if not os.path.exists(host):
                print(f'{pid} {st}: no host shot')
                continue
            ref = os.path.join(out, f'_{pid}-{k}-mock.png')
            subprocess.run(['magick', sheet, '-crop', f'240x240+{88 + 264 * k}+{top_of(idx) + 651}', '+repage', ref], check=True)
            h1 = os.path.join(out, f'_{pid}-{k}-host.png')
            subprocess.run(['magick', host, '-scale', '50%', h1], check=True)
            diff = os.path.join(out, f'_{pid}-{k}-diff.png')
            r = subprocess.run(['magick', 'compare', '-metric', 'AE', '-fuzz', '12%', ref, h1, diff],
                               capture_output=True, text=True)
            ae = float((r.stderr.strip().split() or ['0'])[0])
            print(f'{pid:10s} {st:6s} differ {100.0 * ae / (240 * 240):5.1f}%')
            subprocess.run(['magick', ref, h1, diff, '+append', '-scale', '300%', os.path.join(out, f'{pid}-{k}.png')], check=True)


if __name__ == '__main__':
    main()
