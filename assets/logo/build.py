"""Build flattened SVG files for the Adroit workbench mark.

Every shape is merged with polygon boolean operations into a single path, so
no renderer can show seams between abutting shapes or mask edges.
"""

from io import BytesIO
import os
import sys

import uharfbuzz as hb

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from shapely import affinity
from shapely.geometry import Polygon, box
from shapely.ops import unary_union

INK = "#3b3d38"
PAPER = "#f1ede2"


def workbench():
    """An A-shaped frame and its shared work surface."""
    frame = Polygon([
        (12, 80), (12, 45), (44, 17), (56, 17), (88, 45),
        (88, 80), (74, 80), (74, 51), (50, 30), (26, 51), (26, 80),
    ])
    # A small corner radius echoes the lettering without changing the silhouette.
    return unary_union([frame, box(34, 53, 66, 65)]).buffer(-1, quad_segs=4).buffer(1, quad_segs=4)


def small_workbench(size):
    """Align upright edges and the work surface to the small output grid."""
    if size == 16:
        frame = Polygon([(2, 13), (2, 7), (7, 3), (9, 3), (14, 7),
                         (14, 13), (12, 13), (12, 8), (8, 5), (4, 8), (4, 13)])
        return unary_union([frame, box(5, 8, 11, 10)])
    frame = Polygon([(4, 26), (4, 14.5), (14, 5.5), (18, 5.5), (28, 14.5),
                     (28, 26), (23.5, 26), (23.5, 16.5), (16, 9.75), (8.5, 16.5), (8.5, 26)])
    return unary_union([frame, box(11, 17, 21, 21)])


def geom_to_d(geom):
    polys = getattr(geom, "geoms", [geom])
    out = []
    for p in polys:
        for ring in [p.exterior, *p.interiors]:
            coords = list(ring.coords)[:-1]
            head = coords[0]
            out.append(f"M{head[0]:.2f} {head[1]:.2f}")
            out.extend(f"L{x:.2f} {y:.2f}" for x, y in coords[1:])
            out.append("Z")
    return "".join(out)


def svg(view_w, view_h, body, title):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {view_w:.2f} {view_h:.2f}" '
        f'role="img" aria-label="{title}">\n<title>{title}</title>\n{body}\n</svg>\n'
    )


def path(d, fill):
    return f'<path fill="{fill}" fill-rule="evenodd" d="{d}"/>'


class Wordmark:
    def __init__(self, font_path, weight=600):
        self.font = TTFont(font_path)
        if "fvar" in self.font and any(axis.axisTag == "wght" for axis in self.font["fvar"].axes):
            self.font = instantiateVariableFont(self.font, {"wght": weight}, inplace=True)
        self.glyphs = self.font.getGlyphSet()
        self.upm = self.font["head"].unitsPerEm
        self.order = self.font.getGlyphOrder()
        data = BytesIO()
        self.font.save(data)
        self.shaping_font = hb.Font(hb.Face(data.getvalue()))
        self.shaping_font.scale = (self.upm, self.upm)
        hb.ot_font_set_funcs(self.shaping_font)

    def layout(self, text, size, spacing_em):
        """Return (svg path d, width, ymin, ymax) with the baseline at y = 0."""
        sc = size / self.upm
        buffer = hb.Buffer()
        buffer.add_str(text)
        buffer.guess_segment_properties()
        hb.shape(self.shaping_font, buffer, {"kern": True, "liga": False})
        x = 0.0
        placed = []
        bounds = []
        for info, position in zip(buffer.glyph_infos, buffer.glyph_positions):
            name = self.order[info.codepoint]
            glyph = self.glyphs[name]
            px, py = x + position.x_offset*sc, -position.y_offset*sc
            placed.append((glyph, px, py))
            bp = BoundsPen(self.glyphs)
            glyph.draw(bp)
            if bp.bounds:
                x0, y0, x1, y1 = bp.bounds
                bounds.append((px+x0*sc, py-y1*sc, px+x1*sc, py-y0*sc))
            x += position.x_advance*sc + spacing_em*size
        xmin = min(b[0] for b in bounds)
        xmax = max(b[2] for b in bounds)
        ymin = min(b[1] for b in bounds)
        ymax = max(b[3] for b in bounds)
        pen = SVGPathPen(self.glyphs, ntos=lambda v: f"{v:.2f}")
        for glyph, px, py in placed:
            glyph.draw(TransformPen(pen, (sc, 0, 0, -sc, px-xmin, py)))
        return pen.getCommands(), xmax-xmin, ymin, ymax


def main(out_dir, font_path):
    os.makedirs(out_dir, exist_ok=True)
    mark = workbench()
    mark_d = geom_to_d(mark)

    files = {
        "adroit-mark.svg": svg(100, 100, path(mark_d, INK), "Adroit"),
        "adroit-mark-light.svg": svg(100, 100, path(mark_d, PAPER), "Adroit"),
        "adroit-icon.svg": svg(
            100, 100,
            f'<rect width="100" height="100" rx="23" fill="{INK}"/>\n' + path(mark_d, PAPER),
            "Adroit",
        ),
    }
    for size in (16, 32):
        small_d = geom_to_d(small_workbench(size))
        files[f"adroit-mark-{size}.svg"] = svg(size, size, path(small_d, INK), "Adroit")
        files[f"adroit-mark-{size}-light.svg"] = svg(size, size, path(small_d, PAPER), "Adroit")

    wm = Wordmark(font_path)

    # Keep the lettering size and visible gap from the proportion studies.
    size = 52
    gap = 16
    d, w, ymin, ymax = wm.layout("Adroit", size, 0)
    horizontal_mark = affinity.scale(mark, .6, .6, origin=(0, 0))
    x0, y0, x1, y1 = horizontal_mark.bounds
    hh = max(y1-y0, ymax-ymin) + 16
    ty = hh/2 - (ymin + ymax) / 2
    horizontal_mark = affinity.translate(horizontal_mark, 12-x0, hh/2-(y0+y1)/2)
    text_x = horizontal_mark.bounds[2] + gap
    body = lambda fill: (
        path(geom_to_d(horizontal_mark), fill) + "\n"
        + f'<path fill="{fill}" transform="translate({text_x:.2f} {ty:.2f})" d="{d}"/>'
    )
    hw = text_x + w + 12
    files["adroit-logo-horizontal.svg"] = svg(hw, hh, body(INK), "Adroit")
    files["adroit-logo-horizontal-light.svg"] = svg(hw, hh, body(PAPER), "Adroit")

    # Center the 130% mark in its own square, then separate the lettering below.
    square = 130
    gap = 10
    _, unit_width, _, _ = wm.layout("Adroit", 1, 0.015)
    size = square * .84 / unit_width
    d, w, ymin, ymax = wm.layout("Adroit", size, 0.015)
    vertical_mark = affinity.scale(mark, 1.3, 1.3, origin=(0, 0))
    x0, y0, x1, y1 = vertical_mark.bounds
    vertical_mark = affinity.translate(vertical_mark, square/2-(x0+x1)/2, square/2-(y0+y1)/2)
    vw = max(square, w)
    mx = (vw - square) / 2
    tx = (vw - w) / 2
    ty = square + gap - ymin
    vh = square + gap + (ymax - ymin) + 12
    body = lambda fill: (
        f'<g transform="translate({mx:.2f} 0)">' + path(geom_to_d(vertical_mark), fill) + "</g>\n"
        + f'<path fill="{fill}" transform="translate({tx:.2f} {ty:.2f})" d="{d}"/>'
    )
    files["adroit-logo-vertical.svg"] = svg(vw, vh, body(INK), "Adroit")
    files["adroit-logo-vertical-light.svg"] = svg(vw, vh, body(PAPER), "Adroit")

    for name, content in files.items():
        with open(os.path.join(out_dir, name), "w") as f:
            f.write(content)
        print(name, len(content))


if __name__ == "__main__":
    # The typeface lives in the brand's repository, not here, so it is named.
    main(sys.argv[1], sys.argv[2])
