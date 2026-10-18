"""Build flattened SVG files for the adroit crest (three bound arrows in a ring).

Every shape is merged with polygon boolean operations into a single path, so
no renderer can show seams between abutting shapes or mask edges.
"""

import os
import sys

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from shapely import affinity
from shapely.geometry import LineString, Point, Polygon, box
from shapely.ops import unary_union

INK = "#1a1a1a"
PAPER = "#f3eee2"
SEG = 32


def cubic(p0, p1, p2, p3, n=SEG):
    pts = []
    for i in range(1, n + 1):
        t = i / n
        u = 1 - t
        x = u**3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t**3 * p3[0]
        y = u**3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t**3 * p3[1]
        pts.append((x, y))
    return pts


def mirror(poly):
    return affinity.scale(poly, xfact=-1, yfact=1, origin=(0, 0))


def rounded_rect(x, y, w, h, r):
    return box(x + r, y + r, x + w - r, y + h - r).buffer(r, quad_segs=16)


def annulus(cx, cy, r, width):
    outer = Point(cx, cy).buffer(r + width / 2, quad_segs=96)
    inner = Point(cx, cy).buffer(r - width / 2, quad_segs=96)
    return outer.difference(inner)


def detailed_arrow():
    """Arrow pointing up in local units: barbed head, shaft and two plain vanes.

    The vanes carry no stripes or nock: at logo sizes those details read as
    spikes rather than feathers.
    """
    vane_r = Polygon(
        [(0, 20)] + cubic((0, 20), (2.5, 22), (5.2, 25), (6.4, 30)) + [(6.9, 44), (0, 38)]
    )
    vanes = unary_union([vane_r, mirror(vane_r)])
    shaft = Polygon(
        [(0, -41), (6.8, -26.5), (1.9, -29.38), (1.9, 40), (-1.9, 40), (-1.9, -29.38), (-6.8, -26.5)]
    )
    return unary_union([vanes, shaft])


def small_arrow():
    """Thicker arrow for 32 px: no stripe, no nock."""
    vane_r = Polygon(
        [(0, 22)] + cubic((0, 22), (3, 24), (6.5, 27), (8, 32)) + [(8.6, 44), (0, 38)]
    )
    shaft = Polygon(
        [(0, -43), (10.5, -23), (3.6, -25.63), (3.6, 44), (-3.6, 44), (-3.6, -25.63), (-10.5, -23)]
    )
    return unary_union([vane_r, mirror(vane_r), shaft])


def tiny_arrow():
    """Head and shaft only, for 16 px."""
    return Polygon([(0, -44), (14, -20), (5, -20), (5, 40), (-5, 40), (-5, -20), (-14, -20)])


def place(arrow, angle, scale, cx, cy, shift):
    g = affinity.translate(arrow, 0, shift)
    g = affinity.rotate(g, angle, origin=(0, 0))
    g = affinity.scale(g, scale, scale, origin=(0, 0))
    return affinity.translate(g, cx, cy)


def crest(arrow, *, ring_r, ring_w, scale, angle, shift=-6, knock=None, band=None, band_gap=None):
    cx, cy = 50, 52
    center = place(arrow, 0, scale, cx, cy, shift)
    sides = unary_union([
        place(arrow, -angle, scale, cx, cy, shift),
        place(arrow, angle, scale, cx, cy, shift),
    ])
    if knock:
        sides = sides.difference(center.buffer(knock, quad_segs=8))
    arrows = unary_union([sides, center])
    parts = [annulus(50, 50, ring_r, ring_w)]
    if band_gap:
        arrows = arrows.difference(rounded_rect(*band_gap))
    parts.append(arrows)
    if band:
        parts.append(rounded_rect(*band))
    return unary_union(parts)


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
    def __init__(self, font_path):
        self.font = TTFont(font_path)
        self.glyphs = self.font.getGlyphSet()
        self.cmap = self.font.getBestCmap()
        self.upm = self.font["head"].unitsPerEm

    def layout(self, text, size, spacing_em):
        """Return (svg path d, width, ymin, ymax) with the baseline at y = 0."""
        sc = size / self.upm
        pen = SVGPathPen(self.glyphs, ntos=lambda v: f"{v:.2f}")
        x = 0.0
        ymin, ymax = 0.0, 0.0
        for i, ch in enumerate(text):
            name = self.cmap[ord(ch)]
            glyph = self.glyphs[name]
            glyph.draw(TransformPen(pen, (sc, 0, 0, -sc, x, 0)))
            bp = BoundsPen(self.glyphs)
            glyph.draw(bp)
            if bp.bounds:
                ymin = min(ymin, -bp.bounds[3] * sc)
                ymax = max(ymax, -bp.bounds[1] * sc)
            x += glyph.width * sc
            if i < len(text) - 1:
                x += spacing_em * size
        return pen.getCommands(), x, ymin, ymax


def main(out_dir, font_path):
    os.makedirs(out_dir, exist_ok=True)
    arrow = detailed_arrow()
    mark = crest(
        arrow, ring_r=44, ring_w=6, scale=0.76, angle=32,
        knock=4.6 * 0.76 / 2,
        band=(40, 49.25, 20, 5.5, 1), band_gap=(38.2, 47.45, 23.6, 9.1, 1.6),
    )
    mark_d = geom_to_d(mark)
    icon_mark = crest(
        arrow, ring_r=37, ring_w=5, scale=0.62, angle=32,
        knock=5 * 0.62 / 2,
        band=(42, 49.55, 16, 4.5, 0.8), band_gap=(40.6, 48.15, 18.8, 7.3, 1.3),
    )
    small = crest(small_arrow(), ring_r=43, ring_w=9, scale=0.72, angle=32, knock=2.4)
    tiny = crest(tiny_arrow(), ring_r=42, ring_w=12, scale=0.74, angle=28, shift=-2)

    files = {
        "adroit-mark.svg": svg(100, 100, path(mark_d, INK), "adroit"),
        "adroit-mark-light.svg": svg(100, 100, path(mark_d, PAPER), "adroit"),
        "adroit-icon.svg": svg(
            100, 100,
            f'<rect width="100" height="100" rx="23" fill="{INK}"/>\n' + path(geom_to_d(icon_mark), PAPER),
            "adroit",
        ),
        "adroit-mark-32.svg": svg(100, 100, path(geom_to_d(small), INK), "adroit"),
        "adroit-mark-16.svg": svg(100, 100, path(geom_to_d(tiny), INK), "adroit"),
    }

    wm = Wordmark(font_path)

    # Horizontal lockup: proportions follow the canvas (crest 170, text 84, gap 28, tracking 0.12em).
    size = 84 / 170 * 100
    gap = 28 / 170 * 100
    d, w, ymin, ymax = wm.layout("adroit", size, 0.12)
    ty = 50 - (ymin + ymax) / 2
    body = lambda fill: (
        path(mark_d, fill) + "\n"
        + f'<path fill="{fill}" transform="translate({100 + gap:.2f} {ty:.2f})" d="{d}"/>'
    )
    hw = 100 + gap + w
    files["adroit-logo-horizontal.svg"] = svg(hw, 100, body(INK), "adroit")
    files["adroit-logo-horizontal-light.svg"] = svg(hw, 100, body(PAPER), "adroit")

    # Vertical lockup: crest 150, text 40, gap 20, tracking 0.24em.
    size = 40 / 150 * 100
    gap = 20 / 150 * 100
    d, w, ymin, ymax = wm.layout("adroit", size, 0.24)
    vw = max(100.0, w)
    mx = (vw - 100) / 2
    tx = (vw - w) / 2
    ty = 100 + gap - ymin
    vh = 100 + gap + (ymax - ymin)
    body = lambda fill: (
        f'<g transform="translate({mx:.2f} 0)">' + path(mark_d, fill) + "</g>\n"
        + f'<path fill="{fill}" transform="translate({tx:.2f} {ty:.2f})" d="{d}"/>'
    )
    files["adroit-logo-vertical.svg"] = svg(vw, vh, body(INK), "adroit")
    files["adroit-logo-vertical-light.svg"] = svg(vw, vh, body(PAPER), "adroit")

    for name, content in files.items():
        with open(os.path.join(out_dir, name), "w") as f:
            f.write(content)
        print(name, len(content))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
