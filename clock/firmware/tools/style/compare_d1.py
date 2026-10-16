#!/usr/bin/env python3
"""compare_d1.py - compare.py for the Batch D sheet (boards #31-#35).

    python3 tools/style/compare_d1.py "<Batch D>.html" <preview_dir> <out_dir> [id ...]

Same output as compare.py (<out_dir>/<id>-<k>.png = mock | host | difference at 3x, and
the share of pixels that differ by more than 12 %). Batch D's boards have no outline
around the 1x strip, which sits 660 px below the board's iframe top at x = 88 + 256 k.
"""
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from compare import STATES, boards, shoot  # noqa: E402

IDS = {'#31': 'vumeter', '#32': 'scope', '#33': 'pfd', '#34': 'fuel', '#35': 'monitor'}


def strip_top(full, x, y_guess):
    """The first row at x below y_guess - 10 that is not the sheet's page colour (layout rounding drifts)."""
    r = subprocess.run(['magick', full, '-crop', f'1x30+{x}+{y_guess - 10}', '+repage', 'txt:-'],
                       capture_output=True, text=True, check=True)
    for line in r.stdout.splitlines()[1:]:
        m = re.match(r'0,(\d+):.*#([0-9A-F]{6})', line)
        if m and m.group(2) != 'E7E5DF':
            return y_guess - 10 + int(m.group(1))
    return y_guess


def main():
    if len(sys.argv) < 4:
        print(__doc__)
        sys.exit(1)
    html, prev, out = sys.argv[1:4]
    want = set(sys.argv[4:])
    os.makedirs(out, exist_ok=True)
    bs = boards(html)
    full = os.path.join(out, '_sheet.png')
    if not os.path.exists(full) or os.path.getmtime(full) < os.path.getmtime(html):
        shoot(html, full, bs[-1][1] + bs[-1][2] + 100)
    worst = 0.0
    for title, top, _ in bs:
        pid = IDS.get(title.split()[0])
        if not pid or (want and pid not in want):
            continue
        for k, st in enumerate(STATES):
            host = os.path.join(prev, f'30-{pid}-{k}-{st}.png')
            if not os.path.exists(host):
                print(f'{pid} {st}: no host shot')
                continue
            ref = os.path.join(out, f'_{pid}-{k}-mock.png')
            x0 = 88 + 256 * k
            y0 = strip_top(full, x0 + 120, top + 660)
            subprocess.run(['magick', full, '-crop', f'240x240+{x0}+{y0}', '+repage', ref], check=True)
            h1 = os.path.join(out, f'_{pid}-{k}-host.png')
            subprocess.run(['magick', host, '-scale', '50%', h1], check=True)
            diff = os.path.join(out, f'_{pid}-{k}-diff.png')
            r = subprocess.run(['magick', 'compare', '-metric', 'AE', '-fuzz', '12%', ref, h1, diff],
                               capture_output=True, text=True)
            ae = float((r.stderr.strip().split() or ['0'])[0])
            pct = 100.0 * ae / (240 * 240)
            worst = max(worst, pct)
            subprocess.run(['magick', ref, h1, diff, '+append', '-scale', '300%', os.path.join(out, f'{pid}-{k}.png')], check=True)
            print(f'{pid:10s} {st:6s} differ {pct:5.1f}%')
    print(f'worst {worst:.1f}%')


if __name__ == '__main__':
    main()
