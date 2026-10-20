#!/usr/bin/env python3
"""Render ANSI-colored terminal output into a self-contained SVG "screenshot".

Usage: ansi2svg.py OUT.svg TITLE  < input.ansi
Each input line may start with a literal "$ " to mark a typed prompt line.
"""
import html
import re
import sys

PALETTE = {  # Dracula-ish, readable on GitHub light and dark
    30: "#21222c", 31: "#ff5555", 32: "#50fa7b", 33: "#f1fa8c",
    34: "#bd93f9", 35: "#ff79c6", 36: "#8be9fd", 37: "#f8f8f2",
    90: "#6272a4", 91: "#ff6e6e", 92: "#69ff94", 93: "#ffffa5",
    94: "#d6acff", 95: "#ff92df", 96: "#a4ffff", 97: "#ffffff",
}
FG = "#f8f8f2"
BG = "#282a36"
DIM = "#8a8fa8"
SGR = re.compile(r"\x1b\[([0-9;]*)m")

CHAR_W = 8.4
LINE_H = 18
PAD_X = 20
TOP = 46


def parse(line):
    """Yield (text, fg, bold, dim) runs."""
    fg, bold, dim = None, False, False
    pos = 0
    for m in SGR.finditer(line):
        if m.start() > pos:
            yield line[pos:m.start()], fg, bold, dim
        codes = [int(c) for c in m.group(1).split(";") if c] or [0]
        i = 0
        while i < len(codes):
            c = codes[i]
            if c == 0:
                fg, bold, dim = None, False, False
            elif c == 1:
                bold = True
            elif c == 2:
                dim = True
            elif c == 22:
                bold = dim = False
            elif c == 39:
                fg = None
            elif c in PALETTE:
                fg = PALETTE[c]
            elif c == 38 and i + 2 < len(codes) and codes[i + 1] == 2:
                fg = "#%02x%02x%02x" % tuple(codes[i + 2:i + 5])
                i += 4
            elif c == 38 and i + 2 < len(codes) and codes[i + 1] == 5:
                i += 2
            i += 1
        pos = m.end()
    if pos < len(line):
        yield line[pos:], fg, bold, dim


def main():
    out, title = sys.argv[1], sys.argv[2]
    lines = sys.stdin.read().rstrip("\n").split("\n")
    width_cols = max(len(SGR.sub("", l)) for l in lines)
    w = int(PAD_X * 2 + width_cols * CHAR_W)
    h = int(TOP + len(lines) * LINE_H + 18)
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">',
        "<style>text{font-family:'JetBrains Mono','SFMono-Regular',Menlo,Consolas,'DejaVu Sans Mono',monospace;"
        "font-size:14px;white-space:pre}.b{font-weight:700}</style>",
        f'<rect width="{w}" height="{h}" rx="10" fill="{BG}"/>',
        '<circle cx="20" cy="18" r="6" fill="#ff5f56"/>',
        '<circle cx="40" cy="18" r="6" fill="#ffbd2e"/>',
        '<circle cx="60" cy="18" r="6" fill="#27c93f"/>',
        f'<text x="{w / 2}" y="22" fill="{DIM}" text-anchor="middle" style="font-size:12px">{html.escape(title)}</text>',
    ]
    for n, line in enumerate(lines):
        y = TOP + n * LINE_H
        prompt = line.startswith("$ ")
        spans = []
        col = 0
        for text, fg, bold, dim in parse(line):
            if not text:
                continue
            color = fg or (DIM if dim else FG)
            if prompt and col == 0 and text.startswith("$ "):
                spans.append(f'<tspan x="{PAD_X}" fill="#50fa7b">$</tspan>')
                text = text[2:]
                col += 2
                color = "#f8f8f2"
                bold = True
            cls = ' class="b"' if bold else ""
            op = ' fill-opacity="0.75"' if dim and fg else ""
            x = PAD_X + col * CHAR_W
            tl = len(text) * CHAR_W
            spans.append(
                f'<tspan x="{x:.1f}" textLength="{tl:.1f}" lengthAdjust="spacingAndGlyphs" '
                f'fill="{color}"{cls}{op}>{html.escape(text)}</tspan>'
            )
            col += len(text)
        if spans:
            parts.append(f'<text y="{y}" xml:space="preserve">{"".join(spans)}</text>')
    parts.append("</svg>")
    with open(out, "w") as f:
        f.write("\n".join(parts) + "\n")


if __name__ == "__main__":
    main()
