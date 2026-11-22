"""Builds assets/refined-icons.ttf from Adobe's Spectrum 2 workflow icons.

Usage: python3 assets/build-icons.py <path to @adobe/spectrum-css-workflow-icons>/dist/assets/svg
Needs fontTools. Each icon becomes one glyph at U+F1000 onward, in the
order of ICONS; src/theme.rs refers to them by that codepoint.
"""

import sys
import xml.etree.ElementTree as ET
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.svgLib.path import parse_path

ICONS = [
    "Delete", "Search", "Settings", "GlobeGrid", "Close", "AlertTriangle",
    "Checkmark", "Edit", "Copy", "Refresh", "ChevronDown", "ChevronUp",
    "ListBulleted", "CloseCircle", "Replace", "Add", "Download", "SaveFloppy",
    "Visibility", "Minimize", "Maximize", "Text", "ListMultiSelect",
]
FIRST = 0xF1000
EM, ASCENT, DESCENT = 1000, 800, 200
# Spectrum draws on a 20 unit grid with y pointing down.
SCALE = EM / 20


def glyph(svg: Path):
    pen = TTGlyphPen(None)
    flip = TransformPen(Cu2QuPen(pen, 1.0, reverse_direction=True), (SCALE, 0, 0, -SCALE, 0, ASCENT))
    for element in ET.parse(svg).getroot().iter():
        tag = element.tag.rsplit("}", 1)[-1]
        if tag == "path":
            parse_path(element.get("d"), flip)
        elif tag == "circle":
            cx, cy, r = (float(element.get(k)) for k in ("cx", "cy", "r"))
            k = 0.5523 * r
            flip.moveTo((cx + r, cy))
            flip.curveTo((cx + r, cy + k), (cx + k, cy + r), (cx, cy + r))
            flip.curveTo((cx - k, cy + r), (cx - r, cy + k), (cx - r, cy))
            flip.curveTo((cx - r, cy - k), (cx - k, cy - r), (cx, cy - r))
            flip.curveTo((cx + k, cy - r), (cx + r, cy - k), (cx + r, cy))
            flip.closePath()
        elif tag not in ("svg", "title", "defs", "g"):
            raise ValueError(f"{svg.name}: unsupported element {tag}")
    return pen.glyph()


def main(svg_dir: Path, out: Path):
    names = [".notdef"] + ICONS
    glyphs = {".notdef": TTGlyphPen(None).glyph()}
    glyphs.update({name: glyph(svg_dir / f"S2_Icon_{name}_20_N.svg") for name in ICONS})
    fb = FontBuilder(EM, isTTF=True)
    fb.setupGlyphOrder(names)
    fb.setupCharacterMap({FIRST + i: name for i, name in enumerate(ICONS)})
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics({name: (EM if name != ".notdef" else 0, 0) for name in names})
    fb.setupHorizontalHeader(ascent=ASCENT, descent=-DESCENT)
    fb.setupNameTable({"familyName": "Refined Icons", "styleName": "Regular"})
    fb.setupOS2(sTypoAscender=ASCENT, sTypoDescender=-DESCENT, usWinAscent=ASCENT, usWinDescent=DESCENT)
    fb.setupPost()
    fb.save(out)


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(__file__).with_name("refined-icons.ttf"))
