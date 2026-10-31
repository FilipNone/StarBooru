#!/usr/bin/env python3
"""Draws the StarBooru mark and writes every brand file from it: SVG masters,
app icons, Android resources and apps/app/src/brand_paths.rs.
    python3 -I tools/brand/brand.py
Needs rsvg-convert and magick. Only half of the crown is drawn; the other half is
its mirror, so the mark is symmetrical by construction. See docs/branding.md.

The wordmark is outlines of M PLUS Rounded 1c ExtraBold kept in wordmark.json, so
no font is needed here. To redo them from a font file (needs fontTools; never
commit the font):
    python3 -I tools/brand/brand.py --wordmark-font MPLUSRounded1c-ExtraBold.ttf
"""
import json, math, struct, subprocess, sys, tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
DOCS = ROOT / "docs/brand"
APP = ROOT / "apps/app"
WIDTH = 512  # every mark is this wide, centred on x = 256


def oklch(l, c, h):
    a, b = c * math.cos(math.radians(h)), c * math.sin(math.radians(h))
    lg = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3
    mg = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3
    sg = (l - 0.0894841775 * a - 1.2914855480 * b) ** 3
    rgb = (4.0767416621 * lg - 3.3077115913 * mg + 0.2309699292 * sg,
           -1.2684380046 * lg + 2.6097574011 * mg - 0.3413193965 * sg,
           -0.0041960863 * lg - 0.7034186147 * mg + 1.7076147010 * sg)
    gamma = lambda v: 12.92 * v if v <= 0.0031308 else 1.055 * v ** (1 / 2.4) - 0.055
    return "#" + "".join(f"{round(max(0, min(1, gamma(v))) * 255):02x}" for v in rgb)


# The night is the app's default seed hue (285, apps/app/assets/main.css).
NIGHT = oklch(0.23, 0.085, 285)
NIGHT_TOP = oklch(0.29, 0.105, 285)
NIGHT_LOW = oklch(0.18, 0.07, 285)
GOLD = oklch(0.84, 0.13, 85)
STARLIGHT = oklch(0.97, 0.04, 95)
PAPER = oklch(0.985, 0.008, 285)

# ---- the drawing ------------------------------------------------------------
# Units: the crown's inner arc is about 1 across its radius, the spears fan out
# from the origin, y up. A spear is one flank, tip first: (distance from the
# origin, half width, corner radius, how the edge to the next point bows).
FULL = dict(
    center=[(2.92, 0, .06, .026), (2.13, .30, .07, -.04), (1.84, .195, .05, -.008), (1.60, .13, .16, 0)],
    blade=[(2.50, 0, .055, .022), (1.90, .27, .065, -.036), (1.655, .175, .045, -.006), (1.47, .125, .14, 0)],
    petal=[(2.03, 0, .05, .024), (1.58, .165, .2, -.004), (1.42, .115, .14, 0)],
    # Spear angles from straight up: centre, third (the petal), second, first.
    angles=(0, 20.5, 41, 67), last_valley=54,
    arc=(0, -.12, 1.12), arc_end=61, band=.26, last_valley_extra=.07,
    valley=.17, last_valley_round=.24, wall=0, last_wall=-.004,
    first_scale=.94, first_origin=1.15, end_round=.085, end_bow=-.04,
    # Compass: centre y, long point, short point, waist, then rounding and bows.
    star=(-.12, 1.08, .53, .31, .055, .045, .022, -.028, -.01),
)
# The cut for small sizes: no barbs and more weight, so it survives 16 px.
COMPACT = dict(
    FULL,
    center=[(2.90, 0, .07, .03), (2.05, .33, .12, 0), (1.58, .135, .18, 0)],
    blade=[(2.50, 0, .065, .03), (1.82, .30, .11, 0), (1.46, .125, .15, 0)],
    petal=[(2.03, 0, .06, .03), (1.58, .175, .2, 0), (1.40, .115, .14, 0)],
    angles=(0, 21, 42, 68), last_valley=55,
    arc=(0, -.1, 1.1), arc_end=60, band=.32, last_valley_extra=.05,
    valley=.14, last_valley_round=.2, wall=0, last_wall=0,
    end_round=.1, end_bow=-.03,
    star=(-.14, 1.14, .54, .33, .06, .05, .02, -.03, -.01),
)


def rot(p, deg):
    s, c = math.sin(math.radians(deg)), math.cos(math.radians(deg))
    return (p[0] * c + p[1] * s, -p[0] * s + p[1] * c)


def sub(a, b): return (a[0] - b[0], a[1] - b[1])
def add(a, b): return (a[0] + b[0], a[1] + b[1])
def mul(a, k): return (a[0] * k, a[1] * k)
def lerp(a, b, t): return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)


def unit(a):
    n = math.hypot(*a)
    return (a[0] / n, a[1] / n)


class V:
    """An outline point: its corner radius and the edge leaving it. On a clockwise
    outline bow > 0 bulges outward and bow < 0 scoops in; `arc` is a circle's centre."""

    def __init__(self, p, r=0.0, bow=0.0, arc=None):
        self.p, self.r, self.bow, self.arc = p, r, bow, arc


def edge(a, b, bow, arc):
    if arc is not None:
        ra = math.dist(a, arc)
        a0, a1 = math.atan2(a[1] - arc[1], a[0] - arc[0]), math.atan2(b[1] - arc[1], b[0] - arc[0])
        turn = (a1 - a0 + math.pi) % (2 * math.pi) - math.pi
        h = 4 / 3 * math.tan(turn / 4) * ra
        return (a, add(a, mul((-math.sin(a0), math.cos(a0)), h)), sub(b, mul((-math.sin(a1), math.cos(a1)), h)), b)
    t = unit(sub(b, a))
    q = add(lerp(a, b, .5), mul((-t[1], t[0]), 2 * bow))
    return (a, lerp(a, q, 2 / 3), lerp(b, q, 2 / 3), b)


def split(c, t):
    p01, p12, p23 = lerp(c[0], c[1], t), lerp(c[1], c[2], t), lerp(c[2], c[3], t)
    p012, p123 = lerp(p01, p12, t), lerp(p12, p23, t)
    m = lerp(p012, p123, t)
    return (c[0], p01, p012, m), (m, p123, p23, c[3])


def length(c):
    pts = [split(c, i / 24)[0][3] for i in range(1, 25)]
    return sum(math.dist(a, b) for a, b in zip([c[0]] + pts, pts))


def outline(vs):
    """Closed points -> cubics, each corner rounded to its radius."""
    n = len(vs)
    edges = [edge(vs[i].p, vs[(i + 1) % n].p, vs[i].bow, vs[i].arc) for i in range(n)]
    lens = [length(e) for e in edges]
    cut = []
    for i in range(n):
        # On the chords: a bowed edge must not blow up the cut of a sharp tip.
        a, b = unit(sub(vs[i].p, vs[i - 1].p)), unit(sub(vs[(i + 1) % n].p, vs[i].p))
        turn = math.acos(max(-1, min(1, a[0] * b[0] + a[1] * b[1])))
        cut.append(vs[i].r * math.tan(turn / 2) if vs[i].r > 0 and turn > 1e-4 else 0)
    # All at once: shrinking one corner at a time would treat the two sides differently.
    for _ in range(6):
        fits = [min(1, lens[i] * .98 / (cut[i] + cut[(i + 1) % n])) if cut[i] + cut[(i + 1) % n] > 0 else 1
                for i in range(n)]
        cut = [cut[i] * min(fits[i], fits[i - 1]) for i in range(n)]
    trimmed = []
    for i in range(n):
        e, t1 = edges[i], 1 - cut[(i + 1) % n] / lens[i]
        if t1 < 1:
            e = split(e, t1)[0]
        if cut[i] > 0:
            e = split(e, cut[i] / lens[i] / t1)[1]
        trimmed.append(e)
    segs = []
    for i in range(n):
        a, b = trimmed[i - 1], trimmed[i]
        if cut[i] > 0:
            ta, tb = unit(sub(a[3], a[2])), unit(sub(b[1], b[0]))
            turn = math.acos(max(-1, min(1, ta[0] * tb[0] + ta[1] * tb[1])))
            chord = math.dist(a[3], b[0])
            if turn > 1e-4 and chord > 1e-9:
                h = 4 / 3 * math.tan(turn / 4) * chord / (2 * math.sin(turn / 2))
                segs.append((a[3], add(a[3], mul(ta, h)), sub(b[0], mul(tb, h)), b[0]))
        segs.append(b)
    return [c for c in segs if math.dist(c[0], c[3]) > 1e-6]


def spear(profile, deg, scale=1.0, origin=0.0):
    """Base -> tip -> base. Going up a flank, an edge's bow belongs to its lower point."""
    at = lambda w, r: rot((w * scale, origin + (r - origin) * scale), deg)
    up = [V(at(-profile[i][1], profile[i][0]), profile[i][2] * scale, profile[i - 1][3] * scale)
          for i in range(len(profile) - 1, 0, -1)]
    return up + [V(at(w, r), rad * scale, bow * scale) for r, w, rad, bow in profile]


def ray_circle(deg, cx, cy, radius):
    dx, dy = math.sin(math.radians(deg)), math.cos(math.radians(deg))
    b = dx * cx + dy * cy
    return b + math.sqrt(b * b - (cx * cx + cy * cy - radius * radius))


def crown(s):
    cx, cy, ra = s["arc"]
    a = s["angles"]

    def valley(deg, radius, bow, extra=0.0):
        r = ray_circle(deg, cx, cy, ra + s["band"] + extra)
        return V(rot((0, r), deg), radius, bow)

    flank = len(s["center"]) - 1
    center = spear(s["center"], a[0])
    tip = center[flank]
    half = center[flank + 1:]
    half[-1].bow = s["wall"]
    half.append(valley((a[0] + a[1]) / 2, s["valley"], s["wall"]))
    half += spear(s["petal"], a[1])
    half[-1].bow = s["wall"]
    half.append(valley((a[1] + a[2]) / 2, s["valley"], s["wall"]))
    half += spear(s["blade"], a[2])
    half[-1].bow = s["last_wall"]
    half.append(valley(s["last_valley"], s["last_valley_round"], s["last_wall"], s["last_valley_extra"]))
    # The first spear's outer flank leaves its barb and runs into the band's end.
    half += spear(s["blade"], a[3], s["first_scale"], s["first_origin"])[:-1]
    half[-1].bow = s["end_bow"]
    end = math.radians(s["arc_end"])
    half.append(V((cx + ra * math.sin(end), cy + ra * math.cos(end)), s["end_round"], 0, (cx, cy)))
    # Mirror: an edge belongs to the point it leaves, so reversed points take the
    # previous point's edge.
    left = []
    for i in range(len(half) - 1, -1, -1):
        prev = half[i - 1] if i > 0 else tip
        arc = None if prev.arc is None else (-prev.arc[0], prev.arc[1])
        left.append(V((-half[i].p[0], half[i].p[1]), half[i].r, prev.bow, arc))
    return [tip] + half + [V((0, cy + ra), 0, 0, (-cx, cy))] + left


def star(s):
    cy, long, short, waist, round_long, round_short, round_waist, bow_long, bow_short = s["star"]
    vs = []
    for k in range(8):
        major = k % 2 == 0
        out = rot((0, long if major else short), k * 45)
        between = rot((0, waist), k * 45 + 22.5)
        vs.append(V((out[0], out[1] + cy), round_long if major else round_short, bow_long if major else bow_short))
        vs.append(V((between[0], between[1] + cy), round_waist, bow_short if major else bow_long))
    return vs


def bounds(segs):
    pts = [split(c, i / 16)[0][3] for c in segs for i in range(1, 17)]
    return (min(p[0] for p in pts), min(p[1] for p in pts), max(p[0] for p in pts), max(p[1] for p in pts))


def num(v):
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


class Mark:
    """A cut of the mark in SVG units: WIDTH wide, y down, top at 0."""

    def __init__(self, spec):
        self.crown_segs, self.star_segs = outline(crown(spec)), outline(star(spec))
        c, s = bounds(self.crown_segs), bounds(self.star_segs)
        half = max(abs(c[0]), abs(c[2]), abs(s[0]), abs(s[2]))
        self.k = WIDTH / (2 * half)
        self.top = max(c[3], s[3])
        self.height = math.ceil((self.top - min(c[1], s[1])) * self.k)
        self.crown = self.path(self.crown_segs)
        self.star = self.path(self.star_segs)

    def point(self, p):
        # Round the distance from the axis: both sides of it then get the same number.
        dx = round(round(abs(p[0]) * self.k, 6), 2)
        return (round(WIDTH / 2 + (dx if p[0] > 0 else -dx), 2), round(round((self.top - p[1]) * self.k, 6), 2))

    def path(self, segs):
        pt = lambda p: "{} {}".format(*map(num, self.point(p)))
        return "M" + pt(segs[0][0]) + "".join(f"C{pt(c[1])} {pt(c[2])} {pt(c[3])}" for c in segs) + "Z"

    def check(self):
        for segs in (self.crown_segs, self.star_segs):
            nodes = {self.point(p) for c in segs for p in c}
            for x, y in nodes:
                assert (round(WIDTH - x, 2), y) in nodes, f"not mirrored at {x} {y}"

    def inner(self, crown_fill, star_fill):
        return f'<path fill="{crown_fill}" d="{self.crown}"/><path fill="{star_fill}" d="{self.star}"/>'

    def radius(self):
        """The circle around the mark's box centre that holds it, for Android's safe zone."""
        pts = [self.point(p) for segs in (self.crown_segs, self.star_segs) for c in segs for p in c]
        return max(math.dist(p, (WIDTH / 2, self.height / 2)) for p in pts)


# ---- the wordmark -----------------------------------------------------------
def outline_wordmark(font_path):
    from fontTools.pens.boundsPen import BoundsPen
    from fontTools.pens.svgPathPen import SVGPathPen
    from fontTools.pens.transformPen import TransformPen
    from fontTools.ttLib import TTFont

    font = TTFont(font_path)
    glyphs, cmap = font.getGlyphSet(), font.getBestCmap()
    cap = BoundsPen(glyphs)
    glyphs[cmap[ord("H")]].draw(cap)
    k = 100 / cap.bounds[3]  # capitals are 100 units tall
    tracking = -5.0
    x = 0.0
    placed = []
    for ch in "StarBooru":
        placed.append((cmap[ord(ch)], x))
        x += glyphs[cmap[ord(ch)]].width * k + tracking
    box = BoundsPen(glyphs)
    for g, gx in placed:
        glyphs[g].draw(TransformPen(box, (k, 0, 0, -k, gx, 0)))
    x0, y0, x1, y1 = box.bounds
    pen = SVGPathPen(glyphs, ntos=num)
    for g, gx in placed:
        glyphs[g].draw(TransformPen(pen, (k, 0, 0, -k, gx - x0, -y0)))
    data = {"width": math.ceil(x1 - x0), "height": math.ceil(y1 - y0), "d": pen.getCommands()}
    (HERE / "wordmark.json").write_text(json.dumps(data, indent=1) + "\n")


# ---- files ------------------------------------------------------------------
def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def svg(w, h, body, defs=""):
    d = f"<defs>{defs}</defs>" if defs else ""
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}">{d}{body}</svg>\n'


GRADIENT = (f'<linearGradient id="night" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{NIGHT_TOP}"/>'
            f'<stop offset="1" stop-color="{NIGHT_LOW}"/></linearGradient>')


def tile(mark, inset=0.0, width=.78):
    """The app icon, 1024 square. `inset` leaves a transparent margin (macOS's grid)."""
    side = 1024 * (1 - 2 * inset)
    k = side * width / WIDTH
    x, y = (1024 - WIDTH * k) / 2, (1024 - mark.height * k) / 2
    body = (f'<rect x="{num(1024 * inset)}" y="{num(1024 * inset)}" width="{num(side)}" height="{num(side)}" '
            f'rx="{num(side * .225)}" fill="url(#night)"/>'
            f'<g transform="translate({num(x)} {num(y)}) scale({k:.5f})">{mark.inner(GOLD, STARLIGHT)}</g>')
    return svg(1024, 1024, body, GRADIENT)


def rasterize(svg_text, px, out):
    with tempfile.NamedTemporaryFile("w", suffix=".svg") as f:
        f.write(svg_text)
        f.flush()
        out.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["rsvg-convert", "-w", str(px), "-h", str(px), "-o", str(out), f.name], check=True)


def png_bytes(svg_text, px):
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "i.png"
        rasterize(svg_text, px, out)
        return out.read_bytes()


def ico(entries):
    head = struct.pack("<HHH", 0, 1, len(entries))
    offset = 6 + 16 * len(entries)
    table, blobs = b"", b""
    for px, blob in entries:
        table += struct.pack("<BBBBHHII", px % 256, px % 256, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
        blobs += blob
    return head + table + blobs


def icns(entries):
    body = b"".join(kind.encode() + struct.pack(">I", len(blob) + 8) + blob for kind, blob in entries)
    return b"icns" + struct.pack(">I", len(body) + 8) + body


def android_vector(mark, crown_fill, star_fill):
    k = 66 * .5 / mark.radius()  # 66 of the 108 dp are always visible, as a circle
    x, y = 54 - WIDTH / 2 * k, 54 - mark.height / 2 * k
    return f'''<?xml version="1.0" encoding="utf-8"?>
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="108dp"
    android:height="108dp"
    android:viewportWidth="108"
    android:viewportHeight="108">
    <group android:scaleX="{k:.5f}" android:scaleY="{k:.5f}" android:translateX="{x:.3f}" android:translateY="{y:.3f}">
        <path android:fillColor="{crown_fill}" android:pathData="{mark.crown}" />
        <path android:fillColor="{star_fill}" android:pathData="{mark.star}" />
    </group>
</vector>
'''


def android_res(full, compact):
    res = APP / "android-res"
    write(res / "drawable-v24/ic_launcher_foreground.xml", android_vector(full, GOLD, STARLIGHT))
    write(res / "drawable/ic_launcher_monochrome.xml", android_vector(full, "#FFFFFF", "#FFFFFF"))
    write(res / "drawable/ic_launcher_background.xml", f'''<?xml version="1.0" encoding="utf-8"?>
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    xmlns:aapt="http://schemas.android.com/aapt"
    android:width="108dp"
    android:height="108dp"
    android:viewportWidth="108"
    android:viewportHeight="108">
    <path android:pathData="M0,0h108v108h-108z">
        <aapt:attr name="android:fillColor">
            <gradient android:type="linear" android:startX="54" android:startY="0" android:endX="54" android:endY="108">
                <item android:color="{NIGHT_TOP}" android:offset="0" />
                <item android:color="{NIGHT_LOW}" android:offset="1" />
            </gradient>
        </aapt:attr>
    </path>
</vector>
''')
    write(res / "mipmap-anydpi-v26/ic_launcher.xml", '''<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@drawable/ic_launcher_background" />
    <foreground android:drawable="@drawable/ic_launcher_foreground" />
    <monochrome android:drawable="@drawable/ic_launcher_monochrome" />
</adaptive-icon>
''')
    # dx would name the app after its binary, "StarbooruApp".
    write(res / "values/strings.xml", """<resources>
    <string name="app_name">StarBooru</string>
</resources>
""")
    # Only Android below 8 reads these.
    for name, px in (("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)):
        out = res / f"mipmap-{name}/ic_launcher.webp"
        out.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory() as d:
            png = Path(d) / "i.png"
            rasterize(tile(compact if px < 96 else full), px, png)
            subprocess.run(["magick", str(png), "-define", "webp:lossless=true", str(out)], check=True)


def rust_paths(full, compact, word):
    write(APP / "src/brand_paths.rs", f'''//! Generated by tools/brand/brand.py; change the drawing there.

pub const WIDTH: u32 = {WIDTH};
pub const FULL_HEIGHT: u32 = {full.height};
pub const FULL_CROWN: &str = "{full.crown}";
pub const FULL_STAR: &str = "{full.star}";
pub const COMPACT_HEIGHT: u32 = {compact.height};
pub const COMPACT_CROWN: &str = "{compact.crown}";
pub const COMPACT_STAR: &str = "{compact.star}";
pub const WORDMARK_WIDTH: u32 = {word["width"]};
pub const WORDMARK_HEIGHT: u32 = {word["height"]};
pub const WORDMARK: &str = "{word["d"]}";
''')


def lockup(mark, word, crown_fill, star_fill, word_fill):
    """Mark and name side by side; the name is centred on the mark and its capitals
    are 42% of the mark's height."""
    k = mark.height * .42 / 100
    gap = mark.height * .22
    w = WIDTH + gap + word["width"] * k
    y = (mark.height - word["height"] * k) / 2
    body = (mark.inner(crown_fill, star_fill) +
            f'<path fill="{word_fill}" transform="translate({num(WIDTH + gap)} {num(y)}) scale({k:.4f})" d="{word["d"]}"/>')
    return w, mark.height, svg(num(w), mark.height, body)


def board(full, compact, word):
    """One picture of the whole identity, for docs/branding.md."""
    w, h = 1600, 1180
    place = lambda x, y, width, mark, c, s: (
        f'<g transform="translate({x} {y}) scale({width / WIDTH:.5f})">{mark.inner(c, s)}</g>')
    label = lambda x, y, text, fill="#6b6880": (
        f'<text x="{x}" y="{y}" font-family="sans-serif" font-size="20" fill="{fill}">{text}</text>')
    wordmark = lambda x, y, cap, fill: (
        f'<path fill="{fill}" transform="translate({x} {y}) scale({cap / 100:.4f})" d="{word["d"]}"/>')
    b = [f'<rect width="{w}" height="{h}" fill="{PAPER}"/>',
         f'<rect x="800" width="800" height="620" fill="{NIGHT}"/>',
         place(160, 60, 480, full, NIGHT, NIGHT), place(960, 60, 480, full, GOLD, STARLIGHT)]
    cap = 48
    left = (800 - word["width"] * cap / 100) / 2
    b += [wordmark(num(left), 470, cap, NIGHT), wordmark(num(800 + left), 470, cap, STARLIGHT)]
    b += [label(40, 590, "Full cut, one colour on light"), label(840, 590, "Full cut, gold and starlight on night", "#b9b6d0")]
    x = 40
    for px in (256, 128, 64, 48, 32, 24, 16):
        k = px / 1024
        inner = (tile(full) if px >= 48 else tile(compact, width=.84)).split(">", 1)[1].rsplit("</svg>", 1)[0].replace('id="night"', f'id="n{px}"').replace("#night", f"#n{px}")
        b.append(f'<g transform="translate({x} {660 + 256 - px}) scale({k:.5f})">{inner}</g>')
        x += px + 36
    b.append(label(40, 960, "App icon at 256 to 16 px; the compact cut takes over below 48 px"))
    x = 40
    for name, c in (("Night", NIGHT), ("Gold", GOLD), ("Starlight", STARLIGHT), ("Paper", PAPER)):
        b.append(f'<rect x="{x}" y="1000" width="170" height="110" rx="24" fill="{c}" stroke="#00000022"/>')
        b.append(label(x, 1140, f"{name} {c}"))
        x += 210
    place_x = 960
    for mark, cap_label in ((compact, "32"), (compact, "24")):
        px = int(cap_label)
        k = px / mark.height
        b.append(f'<g transform="translate({place_x} {1040 - px}) scale({k:.5f})">{mark.inner(NIGHT, NIGHT)}</g>')
        place_x += int(WIDTH * k) + 40
    b.append(label(960, 1090, "Compact cut in the interface, one colour"))
    return svg(w, h, "".join(b))


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--wordmark-font":
        outline_wordmark(sys.argv[2])
    full, compact = Mark(FULL), Mark(COMPACT)
    full.check()
    compact.check()
    word = json.loads((HERE / "wordmark.json").read_text())

    for name, mark, c, s in (("mark", full, NIGHT, NIGHT), ("mark-gold", full, GOLD, STARLIGHT),
                             ("mark-compact", compact, NIGHT, NIGHT), ("mark-compact-gold", compact, GOLD, STARLIGHT)):
        write(DOCS / f"{name}.svg", svg(WIDTH, mark.height, mark.inner(c, s)))
    write(DOCS / "wordmark.svg", svg(word["width"], word["height"], f'<path fill="{NIGHT}" d="{word["d"]}"/>'))
    write(DOCS / "lockup.svg", lockup(full, word, NIGHT, NIGHT, NIGHT)[2])
    write(DOCS / "lockup-gold.svg", lockup(full, word, GOLD, STARLIGHT, STARLIGHT)[2])
    icon = tile(full)
    small = tile(compact, width=.84)
    write(DOCS / "app-icon.svg", icon)
    subprocess.run(["rsvg-convert", "-o", str(DOCS / "board.png"), "/dev/stdin"], input=board(full, compact, word).encode(), check=True)

    web = APP / "assets/brand"
    write(web / "favicon.svg", small)
    rasterize(small, 32, web / "favicon-32.png")
    rasterize(icon, 180, web / "apple-touch-icon.png")
    rasterize(icon, 256, web / "window-icon.png")

    icons = APP / "icons"
    for px in (32, 128, 256, 512):
        rasterize(small if px < 48 else icon, px, icons / f"icon-{px}.png")
    (icons / "icon.ico").write_bytes(ico([(px, png_bytes(small if px < 48 else icon, px)) for px in (16, 24, 32, 48, 64, 128, 256)]))
    mac = tile(full, inset=100 / 1024)
    (icons / "icon.icns").write_bytes(icns([(k, png_bytes(mac, px)) for k, px in (("ic07", 128), ("ic08", 256), ("ic09", 512), ("ic10", 1024))]))

    android_res(full, compact)
    rust_paths(full, compact, word)


if __name__ == "__main__":
    main()
