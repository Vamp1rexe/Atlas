#!/usr/bin/env python3
"""Generates the built-in Atlas cosmetics: models, textures, previews and catalog.json.

Run from the repo root:  python3 cosmetics/tools/gen_cosmetics.py
Needs Pillow. Add a new cosmetic by writing a function that returns a `Design`
and listing it in DESIGNS at the bottom.

The preview renderer below mirrors the in-game math (bone chain, JOML rotationXYZ,
sin() animation), so the PNG/GIF previews show the real flap, not an approximation.
"""
import json
import math
import os
import random
from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
os.chdir(ROOT)

CLEAR = (0, 0, 0, 0)


def lerp(a, b, t):
    return tuple(round(a[k] + (b[k] - a[k]) * t) for k in range(3))


def mul(c, f):
    return tuple(max(0, min(255, round(v * f))) for v in c)


def hex_rgb(h):
    h = (h or "").lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4)) if len(h) == 6 else (255, 255, 255)


# ---------------------------------------------------------------- texture atlas
class Atlas:
    def __init__(self, width):
        self.w, self.h, self.x, self.y, self.rowh = width, 0, 0, 0, 0
        self.items = []

    def alloc(self, w, h, painter):
        w, h = int(w), int(h)
        if self.x + w > self.w:
            self.x, self.y, self.rowh = 0, self.y + self.rowh + 1, 0
        pos = (self.x, self.y)
        self.x += w + 1
        self.rowh = max(self.rowh, h)
        self.h = max(self.h, self.y + h)
        self.items.append((pos, w, h, painter))
        return (pos[0], pos[1], pos[0] + w, pos[1] + h)

    def image(self):
        img = Image.new("RGBA", (self.w, max(self.h, 1)), CLEAR)
        px = img.load()
        for (x0, y0), w, h, painter in self.items:
            for j in range(h):
                for i in range(w):
                    px[x0 + i, y0 + j] = painter(i, j, w, h)
        return img


# ---------------------------------------------------------------- bones
class Rect:
    """Flat slab in the bone's local frame (y down). The visible face sits at z1."""

    def __init__(self, x0, y0, x1, y1, z0, z1, uv):
        self.x0, self.y0, self.x1, self.y1, self.z0, self.z1, self.uv = x0, y0, x1, y1, z0, z1, uv


class Bone:
    def __init__(self, name, parent, rel, rot_z=0.0, base_x=0.0, base_y=0.0, amp=(0, 0, 0),
                 move=(0, 0, 0), speed=2.0, phase=0.0, rects=None):
        self.name, self.parent, self.rel = name, parent, rel   # rel: pivot offset in the parent's frame
        self.rot_z, self.base_x, self.base_y = rot_z, base_x, base_y
        self.amp, self.move, self.speed, self.phase = amp, move, speed, phase
        self.rects = rects or []


def _rename(n):
    return n[:-2] + "_r" if n.endswith("_l") else n + "_r"


def mirror(bones):
    """Left side to right side: x -> -x, texture flipped, y/z rotations negated."""
    out = []
    for b in bones:
        out.append(Bone(
            _rename(b.name), _rename(b.parent) if b.parent else None,
            (-b.rel[0], b.rel[1], b.rel[2]), -b.rot_z, b.base_x, -b.base_y,
            (b.amp[0], -b.amp[1], -b.amp[2]), (b.move[0], -b.move[1], -b.move[2]),
            b.speed, b.phase,
            [Rect(-r.x1, r.y0, -r.x0, r.y1, r.z0, r.z1, (r.uv[2], r.uv[1], r.uv[0], r.uv[3])) for r in b.rects]))
    return out


class Design:
    def __init__(self, id, name, slot, rarity, description, attach, atlas, bones,
                 default_color="", scale=1.0, art_from_behind=False, speed=2.0, card_scale=1.4):
        self.id, self.name, self.slot, self.rarity, self.description = id, name, slot, rarity, description
        self.attach, self.atlas, self.bones = attach, atlas, bones
        self.default_color, self.scale, self.art_from_behind = default_color, scale, art_from_behind
        self.speed, self.card_scale = speed, card_scale

    def pivots(self):
        piv = {}
        for b in self.bones:
            pp = piv[b.parent] if b.parent else (0.0, 0.0, 0.0)
            piv[b.name] = (pp[0] + b.rel[0], pp[1] + b.rel[1], pp[2] + b.rel[2])
        return piv

    def model(self):
        piv = self.pivots()
        bones = []
        for b in self.bones:
            p = piv[b.name]
            cubes = []
            for r in b.rects:
                rev = [r.uv[2], r.uv[1], r.uv[0], r.uv[3]]
                # One face is enough: the mod draws every quad double-sided.
                faces = {"south": list(r.uv) if self.art_from_behind else rev}
                cubes.append({
                    "from": [round(p[0] + r.x0, 3), round(p[1] + r.y0, 3), round(p[2] + r.z0, 3)],
                    "to": [round(p[0] + r.x1, 3), round(p[1] + r.y1, 3), round(p[2] + r.z1, 3)],
                    "faces": faces,
                })
            bone = {"name": b.name, "parent": b.parent, "pivot": [round(v, 3) for v in p]}
            bone["rotation"] = {
                "base": [round(b.base_x, 3), round(b.base_y, 3), round(b.rot_z, 3)],
                "amp": list(b.amp), "move": list(b.move), "speed": b.speed, "phase": round(b.phase, 3),
            }
            if cubes:
                bone["cubes"] = cubes
            bones.append(bone)
        return {"format": 2, "texture_size": [self.atlas.w, self.atlas.h], "attach": self.attach,
                "offset": [0, 0, 0], "scale": self.scale, "bones": bones}


# ---------------------------------------------------------------- quad budget
def merged_quads(img):
    """Same greedy rectangle merge the mod does at load time."""
    w, h = img.size
    px = img.load()
    done = [[False] * w for _ in range(h)]
    n = 0
    for j in range(h):
        for i in range(w):
            c = px[i, j]
            if done[j][i] or c[3] == 0:
                continue
            cw = 1
            while i + cw < w and not done[j][i + cw] and px[i + cw, j] == c:
                cw += 1
            ch = 1
            while j + ch < h and all(not done[j + ch][k] and px[k, j + ch] == c for k in range(i, i + cw)):
                ch += 1
            for y in range(j, j + ch):
                for x in range(i, i + cw):
                    done[y][x] = True
            n += 1
    return n


def count_quads(design):
    img = design.atlas.image()
    total = 0
    for b in design.bones:
        for r in b.rects:
            u0, v0, u1, v1 = r.uv
            total += merged_quads(img.crop((min(u0, u1), min(v0, v1), max(u0, u1), max(v0, v1))))
    return total


# ---------------------------------------------------------------- 3d preview (mirrors the mod)
def matmul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(3)) for j in range(3)] for i in range(3)]


def matvec(m, v):
    return [sum(m[i][k] * v[k] for k in range(3)) for i in range(3)]


def rot_xyz(ax, ay, az):
    """JOML Quaternionf.rotationXYZ: M = Rx * Ry * Rz (z is applied first). Degrees."""
    ax, ay, az = (math.radians(a) for a in (ax, ay, az))
    cx, sx, cy, sy, cz, sz = math.cos(ax), math.sin(ax), math.cos(ay), math.sin(ay), math.cos(az), math.sin(az)
    rx = [[1, 0, 0], [0, cx, -sx], [0, sx, cx]]
    ry = [[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]]
    rz = [[cz, -sz, 0], [sz, cz, 0], [0, 0, 1]]
    return matmul(matmul(rx, ry), rz)


IDENT = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]


SKEW = 0.42


def shape(x):
    """Same skewed sine as the mod: snaps through the centre, lingers at the turning points."""
    return math.sin(x + SKEW * math.sin(x))


def pose_bones(design, t, move=0.0):
    """world (R, p, s) per bone; t is the mod's animation clock, move 0..1 the smoothed walking factor."""
    amp_mul = 0.55 + 0.45 * move
    out = {}
    for b in design.bones:
        if b.parent:
            rp, pp, s = out[b.parent]
            off = matvec(rp, b.rel)
            p = [pp[k] + s * off[k] for k in range(3)]
        else:
            rp, s = IDENT, design.scale
            p = list(b.rel)
        wave = shape(t * b.speed + b.phase) * amp_mul
        ax = b.base_x + b.amp[0] * wave + b.move[0] * move
        ay = b.base_y + b.amp[1] * wave + b.move[1] * move
        az = b.rot_z + b.amp[2] * wave + b.move[2] * move
        out[b.name] = (matmul(rp, rot_xyz(ax, ay, az)), p, s)
    return out


def _proj(p, phi, cx, cy, sc):
    c, s = math.cos(phi), math.sin(phi)
    x1 = p[0] * c + p[2] * s
    z1 = -p[0] * s + p[2] * c
    return (cx - x1 * sc, cy + p[1] * sc), z1


def _tinted(img, tint):
    if not tint or tint == (255, 255, 255):
        return img
    out = img.copy()
    px = out.load()
    for j in range(out.height):
        for i in range(out.width):
            r, g, b, a = px[i, j]
            if a:
                px[i, j] = (r * tint[0] // 255, g * tint[1] // 255, b * tint[2] // 255, a)
    return out


def _tex_crop(img, uv):
    u0, v0, u1, v1 = uv
    crop = img.crop((min(u0, u1), min(v0, v1), max(u0, u1), max(v0, v1)))
    return crop.transpose(Image.FLIP_LEFT_RIGHT) if u1 < u0 else crop


def _paste_quad(img, crop, pa, pb, pc):
    w, h = crop.size
    ux, uy = (pb[0] - pa[0]) / w, (pb[1] - pa[1]) / w
    vx, vy = (pc[0] - pa[0]) / h, (pc[1] - pa[1]) / h
    det = ux * vy - uy * vx
    if abs(det) < 1e-6:
        return
    pd = (pb[0] + pc[0] - pa[0], pb[1] + pc[1] - pa[1])
    xs, ys = (pa[0], pb[0], pc[0], pd[0]), (pa[1], pb[1], pc[1], pd[1])
    x0, y0 = max(0, math.floor(min(xs)) - 1), max(0, math.floor(min(ys)) - 1)
    x1, y1 = min(img.width, math.ceil(max(xs)) + 1), min(img.height, math.ceil(max(ys)) + 1)
    if x1 <= x0 or y1 <= y0:
        return
    a, b, d, e = vy / det, -vx / det, -uy / det, ux / det
    c = a * (x0 - pa[0]) + b * (y0 - pa[1])
    f = d * (x0 - pa[0]) + e * (y0 - pa[1])
    out = crop.transform((x1 - x0, y1 - y0), Image.AFFINE, (a, b, c, d, e, f), resample=Image.NEAREST)
    img.alpha_composite(out, (x0, y0))


BODY = [  # head, torso, arms, legs in model space (y down)
    ((-4, 12, -2, 0, 24, 2), (58, 64, 76)), ((-0, 12, -2, 4, 24, 2), (58, 64, 76)),
    ((-4, 0, -2, 4, 12, 2), (76, 84, 98)),
    ((4, 0, -2, 8, 12, 2), (68, 76, 90)), ((-8, 0, -2, -4, 12, 2), (68, 76, 90)),
    ((-4, -8, -4, 4, 0, 4), (96, 104, 118)),
]


def _draw_box(d, box, col, phi, cx, cy, sc):
    x0, y0, z0, x1, y1, z1 = box[0], box[1], box[2], box[3], box[4], box[5]
    faces = [
        ((0, 0, 1), [(x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)], 1.0),
        ((0, 0, -1), [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0)], 0.78),
        ((1, 0, 0), [(x1, y0, z0), (x1, y0, z1), (x1, y1, z1), (x1, y1, z0)], 0.88),
        ((-1, 0, 0), [(x0, y0, z0), (x0, y0, z1), (x0, y1, z1), (x0, y1, z0)], 0.88),
        ((0, -1, 0), [(x0, y0, z0), (x1, y0, z0), (x1, y0, z1), (x0, y0, z1)], 1.12),
        ((0, 1, 0), [(x0, y1, z0), (x1, y1, z0), (x1, y1, z1), (x0, y1, z1)], 0.7),
    ]
    c, s = math.cos(phi), math.sin(phi)
    vis = []
    for n, pts, shade in faces:
        if -n[0] * s + n[2] * c <= 0.01:
            continue
        proj = [_proj(p, phi, cx, cy, sc) for p in pts]
        vis.append((sum(z for _, z in proj) / 4, [q for q, _ in proj], shade))
    for _, poly, shade in sorted(vis, key=lambda v: v[0]):
        d.polygon(poly, fill=mul(col, shade) + (255,))


def render(design, t=0.0, move=0.0, size=(400, 400), scale=5.0, phi=0.0, bg=(22, 24, 29, 255),
           tint=None, body=True, cy_frac=0.36):
    ss = 2
    w, h = size[0] * ss, size[1] * ss
    img = Image.new("RGBA", (w, h), bg)
    cx, cy, sc = w / 2, h * cy_frac, scale * ss
    if body:
        d = ImageDraw.Draw(img)
        c, s = math.cos(phi), math.sin(phi)
        order = sorted(BODY, key=lambda b: -(-((b[0][0] + b[0][3]) / 2) * s + ((b[0][2] + b[0][5]) / 2) * c))
        for box, col in order:
            _draw_box(d, box, col, phi, cx, cy, sc)
    tex = _tinted(design.atlas.image(), tint)
    poses = pose_bones(design, t, move)
    items = []
    for b in design.bones:
        rmat, p, s = poses[b.name]
        for r in b.rects:
            def world(x, y, z):
                v = matvec(rmat, (x, y, z))
                return [p[k] + s * v[k] for k in range(3)]
            za = r.z1
            wa, wb, wc = world(r.x0, r.y0, za), world(r.x1, r.y0, za), world(r.x0, r.y1, za)
            wd = world((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2, za)
            (pa, _), (pb, _), (pc, _), (_, depth) = (_proj(q, phi, cx, cy, sc) for q in (wa, wb, wc, wd))
            items.append((depth, pa, pb, pc, r.uv))
    for _, pa, pb, pc, uv in sorted(items, key=lambda i: i[0]):
        _paste_quad(img, _tex_crop(tex, uv), pa, pb, pc)
    return img.resize(size, Image.LANCZOS)


# ---------------------------------------------------------------- painters
# Painters work in model units (1 unit = 1/16 block). WingBuilder.rect() samples them
# DENSITY times per unit, so the textures carry finer detail than the old 1-texel look.
DENSITY = 2


def q(f, step=0.05):
    """Quantised brightness factor: keeps the number of distinct colours (and quads) small."""
    return round(f / step) * step


def fract(x):
    return x - math.floor(x)


def hash2(a, b):
    return fract(math.sin(a * 127.1 + b * 311.7) * 43758.5453)


def bone_painter(L, th, pal, taper=0.45):
    def paint(u, v, W, H):
        t = u / L
        hw = th / 2.0 * (1 - taper * t)
        d = v - th / 2.0
        # rounded knuckle at the root end
        if u < th * 0.5:
            hw = max(hw, math.sqrt(max(0.0, (th * 0.5) ** 2 - (th * 0.5 - u) ** 2)))
        if abs(d) > hw:
            return CLEAR
        col = pal["hi"] if d < -hw * 0.25 else (pal["dark"] if d > hw * 0.35 else pal["mid"])
        if (u % 4.0) < 0.5 and u > 1.5:      # joint rings
            col = mul(col, 0.82)
        return col + (255,)
    return paint


def claw_painter(L, h, pal, bend=0.0):
    def paint(u, v, W, H):
        t = u / L
        c = h / 2.0 + bend * h * t * t
        hw = max(0.3, (h / 2.0) * (1 - t) ** 0.9)
        d = v - c
        if abs(d) > hw:
            return CLEAR
        col = pal["hi"] if d < -0.1 else (pal["dark"] if d > 0.3 else pal["mid"])
        if t > 0.85:
            col = mul(col, 1.0)
        return col + (255,)
    return paint


def feather_painter(L, w, kind, pal, k, fine=True):
    half = w / 2.0
    jit = 1.0 - 0.035 * (k % 3)
    slope = 0.16 + 0.03 * (k % 3)          # barbs sweep toward the tip

    def paint(u, v, W, H):
        t = u / L
        d = v - half
        ad = abs(d)
        if kind == "round":
            prof = min(1.0, 0.5 + t * 2.2)
            if t > 0.7:
                prof *= math.sqrt(max(0.0, 1 - ((t - 0.7) / 0.3) ** 2))
        else:
            prof = min(1.0, 0.42 + t * 3.2)
            if t > 0.5:
                prof *= max(0.0, 1 - (t - 0.5) / 0.5) ** 0.8
        hw = half * prof
        if ad > hw:
            return CLEAR
        # split barbs along the outer edge, more towards the tip
        if fine and hw > 1.4 and ad > hw - 0.75 and t > 0.3:
            if fract(u * 0.55 + k * 0.37 + (0.5 if d > 0 else 0.0)) < 0.2 * t:
                return CLEAR
        if ad < 0.28:                                  # rachis
            col = pal["rachis"]
        else:
            vane = (ad - 0.28) / max(0.01, hw - 0.28)  # 0 at the shaft, 1 at the edge
            base = pal["light"] if d < 0 else pal["base"]
            # barbs: fine stripes angled towards the tip
            stripe = fract((ad * 1.7 + u * slope) * 0.9)
            f = 1.0 + ((0.045 if stripe < 0.5 else -0.045) if fine else 0.0)
            f *= 1.0 - 0.10 * vane ** 2                # softly darker toward the vane edge
            f *= 0.9 + 0.1 * min(1.0, t * 4.0)         # darker down at the base
            col = mul(base, q(f, 0.03 if fine else 0.06))
            if ad > hw - 0.4:
                col = lerp(col, pal["dark"], 0.55)
            if t > 0.8:
                col = lerp(col, pal["tip"], min(1.0, (t - 0.8) / 0.2) * 0.7)
        return mul(col, jit) + (255,)
    return paint


# ---------------------------------------------------------------- wing builder
class WingBuilder:
    """Bones are placed by absolute rest angle (degrees, y down: -90 is up, 90 is down)."""

    def __init__(self, atlas, speed):
        self.atlas, self.speed = atlas, speed
        self.bones, self.abs, self.len, self.phase = [], {}, {}, {}
        self.n = 0

    def bone(self, name, parent, t, angle, length, rects, shoulder=None, base_x=0.0, base_y=0.0,
             amp=(0, 0, 0), move=(0, 0, 0), phase=0.0):
        if parent is None:
            rel, rot = shoulder, angle
        else:
            rel, rot = (t * self.len[parent], 0.0, 0.0), angle - self.abs[parent]
        self.bones.append(Bone(name, parent, rel, rot, base_x, base_y, amp, move, self.speed, phase, rects))
        self.abs[name], self.len[name], self.phase[name] = angle, length, phase
        return self.bones[-1]

    def rect(self, x0, y0, x1, y1, z, painter):
        D = DENSITY
        wu, hu = x1 - x0, y1 - y0

        def sample(i, j, W, H):
            return painter((i + 0.5) / D, (j + 0.5) / D, wu, hu)
        uv = self.atlas.alloc(math.ceil(wu * D), math.ceil(hu * D), sample)
        return Rect(x0, y0, x0 + math.ceil(wu * D) / D, y0 + math.ceil(hu * D) / D, z - 0.3, z, uv)

    def bone_rect(self, L, th, z, pal, taper=0.45):
        return self.rect(0, -th / 2.0, L, th / 2.0, z, bone_painter(L, th, pal, taper))

    def claw_rect(self, x0, L, h, z, pal, bend=0.0):
        return self.rect(x0, -h / 2.0, x0 + L, h / 2.0, z, claw_painter(L, h, pal, bend))

    def feather(self, parent, t, angle, L, w, kind, pal, z):
        self.n += 1
        name = f"f{self.n}_l"
        rect = self.rect(0.5, -w / 2.0, 0.5 + L, w / 2.0, z, feather_painter(L, w, kind, pal, self.n, fine=z >= 1.4))
        self.bone(name, parent, t, angle, L, [rect], amp=(0, 1.5 + 0.06 * L, 2.2),
                  phase=self.phase[parent] + 0.3 + 0.35 * t)

    def membrane(self, host, host_len, partner_len, delta, pal, bite, jag, holes, ribs, seed, z=0.5):
        """Wedge of skin between this finger and the next one, with a concave, optionally ragged edge."""
        d = math.radians(delta)
        a = (host_len, 0.0)
        b = (partner_len * math.cos(d), partner_len * math.sin(d))
        rnd = random.Random(seed)
        mx, my = (a[0] + b[0]) / 2, (a[1] + b[1]) / 2
        ml = math.hypot(mx, my)
        ux, uy = -mx / ml, -my / ml
        pts = [(0.0, 0.0), a]
        n = 14
        for s in range(1, n):
            f = s / n
            off = bite * math.sin(math.pi * f)
            if jag:
                off += rnd.uniform(-jag, jag) * math.sqrt(math.sin(math.pi * f))
            off = max(0.0, off)
            pts.append((a[0] * (1 - f) + b[0] * f + ux * off, a[1] * (1 - f) + b[1] * f + uy * off))
        pts.append(b)
        x0, y0 = math.floor(min(p[0] for p in pts)), math.floor(min(p[1] for p in pts))
        x1, y1 = math.ceil(max(p[0] for p in pts)), math.ceil(max(p[1] for p in pts))
        D = DENSITY
        W, H = (x1 - x0) * D, (y1 - y0) * D
        ss = 3
        m = Image.new("L", (W * ss, H * ss), 0)
        ImageDraw.Draw(m).polygon([((px - x0) * D * ss, (py - y0) * D * ss) for px, py in pts], fill=255)
        m = m.resize((W, H), Image.BOX).load()
        mask = [[m[i, j] >= 128 for i in range(W)] for j in range(H)]
        for _ in range(holes):
            f, r = rnd.uniform(0.28, 0.72), rnd.uniform(0.5, 0.78) * host_len
            hx, hy, hr = (r * math.cos(f * d) - x0) * D, (r * math.sin(f * d) - y0) * D, rnd.uniform(1.3, 2.2) * D
            for j in range(H):
                for i in range(W):
                    if math.hypot(i + 0.5 - hx, j + 0.5 - hy) <= hr:
                        mask[j][i] = False

        def inside(i, j):
            return 0 <= i < W and 0 <= j < H and mask[j][i]

        def rim_dist(i, j):
            """1 for the outermost texel ring, 2 for the next, ... (capped at 3)."""
            for k in (1, 2):
                if not all(inside(i + dx * k, j + dy * k) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                    return k
            return 3

        def paint(i, j, W_, H_):
            if not mask[j][i]:
                return CLEAR
            rd = rim_dist(i, j)
            if rd == 1:
                return pal["edge"] + (255,)
            px, py = x0 + (i + 0.5) / D, y0 + (j + 0.5) / D
            f, r = math.atan2(py, px) / d, math.hypot(px, py)
            if f < 0.05 or f > 0.95:                       # runs along the finger bones
                return pal["vein"] + (255,)
            if rd == 2:
                return lerp(pal["edge"], pal["dark"], 0.5) + (255,)
            if ribs:
                # overlapping scales in staggered rows, bigger towards the rim
                sc = 3.4
                row = math.floor(r / sc)
                col_i = math.floor((f * d * r) / sc + (0.5 if row % 2 else 0.0))
                cu = fract(r / sc)
                sh = 0.90 + 0.12 * hash2(col_i, row)
                col = pal["light"] if cu > 0.72 else (pal["base"] if cu > 0.18 else pal["dark"])
                col = mul(col, q(sh, 0.04))
                if (f * d * r / sc + (0.5 if row % 2 else 0.0)) % 1.0 < 0.08:
                    col = lerp(col, pal["vein"], 0.5)
                return col + (255,)
            # leathery skin: veins fanning out from the wrist, fainter between them
            col = pal["dark"] if r < host_len * 0.28 else (pal["light"] if r > host_len * 0.78 else pal["base"])
            col = mul(col, q(0.94 + 0.06 * (r / host_len), 0.03))
            for vf, strength in ((0.25, 0.35), (0.5, 0.55), (0.75, 0.35)):
                wob = 0.012 * math.sin(r * 0.5 + vf * 9)
                if abs(f - vf - wob) < 0.5 * 0.55 / max(1.0, r) * 1.4 and r > 4:
                    col = lerp(col, pal["vein"], strength)
            return col + (255,)
        return self.rect(x0, y0, x1, y1, z, lambda u, v, a_, b_: paint(min(W - 1, int(u * D)), min(H - 1, int(v * D)), 0, 0))


SHOULDER = (2.5, 3.0, 2.3)


def finish(b, id, name, rarity, desc, color, scale, speed, card):
    bones = b.bones + mirror(b.bones)
    return Design(id, name, "wings", rarity, desc, "body", b.atlas, bones, color, scale, False, speed, card)


# ---------------------------------------------------------------- designs
def angel():
    b = WingBuilder(Atlas(512), speed=2.2)
    gold = dict(hi=(255, 228, 150), mid=(228, 180, 84), dark=(160, 112, 44))

    def pal(f):
        return dict(base=mul((244, 246, 252), f), light=mul((255, 255, 255), f), dark=mul((204, 212, 232), f),
                    rachis=mul((255, 243, 214), f), tip=mul((182, 196, 232), f))
    back, mid, front = pal(0.93), pal(0.98), pal(1.0)
    A_ARM, A_FORE, A_HAND = -64, -30, -8

    b.bone("arm_l", None, 0, A_ARM, 11, [b.bone_rect(11, 3, 3.2, gold, 0.3)], shoulder=SHOULDER,
           base_y=-24, amp=(0, 15, 3), move=(0, -10, 0), phase=0.0)
    b.bone("fore_l", "arm_l", 1.0, A_FORE, 13, [b.bone_rect(13, 3, 3.2, gold, 0.3)], amp=(0, 9, 3), phase=0.5)
    b.bone("hand_l", "fore_l", 1.0, A_HAND, 8, [b.bone_rect(8, 2, 3.2, gold, 0.5)], amp=(0, 8, 4), phase=1.0)

    sec_t = [0.08, 0.24, 0.4, 0.56, 0.72, 0.9]
    sec_a = [92, 85, 78, 71, 64, 57]
    for t, a, L in zip(sec_t, sec_a, [22, 22, 21, 20, 18, 16]):
        b.feather("fore_l", t, a, L, 7, "round", back, 0.2)
    pri_t = [1.0, 0.84, 0.68, 0.52, 0.36, 0.2, 0.06]
    pri_a = [-2, 12, 26, 40, 53, 65, 76]
    for t, a, L in zip(pri_t, pri_a, [24, 28, 29, 28, 26, 23, 20]):
        b.feather("hand_l", t, a, L, 5, "point", mid, 0.9)
    for t, a, L in zip(pri_t[:6], pri_a[:6], [10, 12, 12, 12, 11, 10]):
        b.feather("hand_l", t, a, L, 5, "round", front, 1.5)
    for t, a, L in zip(sec_t, sec_a, [11, 11, 11, 10, 10, 9]):
        b.feather("fore_l", t, a - 3, L, 7, "round", front, 1.6)
    for t, a in zip([0.1, 0.3, 0.5, 0.7, 0.9], [96, 95, 94, 93, 92]):
        b.feather("fore_l", t, a, 6, 5, "round", front, 2.1)
    for t in (0.5, 0.78, 1.0):
        b.feather("arm_l", t, 98, 6, 5, "round", front, 2.1)
    for t, a, L in zip([0.15, 0.35, 0.55, 0.75, 0.95], [104, 99, 94, 89, 84], [11, 13, 14, 13, 11]):
        b.feather("arm_l", t, a, L, 7, "round", front, 2.8)
    for a, L in zip([-58, -40], [8, 7]):
        b.feather("fore_l", 1.0, a, L, 3, "point", front, 1.9)
    return finish(b, "wings_angel", "Engelsflügel", "epic",
                  "Gefiederte Flügel mit goldenem Rahmen.", "#ffffff", 0.72, 2.2, 1.9)


MEM_DEMON = dict(base=(204, 204, 204), light=(236, 236, 236), dark=(152, 152, 152), vein=(116, 116, 116), edge=(86, 86, 86))
MEM_DRAGON = dict(base=(184, 184, 184), light=(218, 218, 218), dark=(134, 134, 134), vein=(98, 98, 98), edge=(70, 70, 70))
BONE_DEMON = dict(hi=(216, 216, 216), mid=(160, 160, 160), dark=(104, 104, 104))
BONE_DRAGON = dict(hi=(204, 204, 204), mid=(148, 148, 148), dark=(92, 92, 92))
CLAW = dict(hi=(255, 255, 255), mid=(226, 226, 226), dark=(156, 156, 156))


def bat_wing(id, name, rarity, desc, color, scale, speed, card, arm, fore, a_arm, a_fore, fingers, anchor,
             mem, bone, th, bite, jag, holes, ribs, spikes, seed):
    b = WingBuilder(Atlas(512), speed)
    arm_len, fore_len = arm, fore
    b.bone("arm_l", None, 0, a_arm, arm_len, [b.bone_rect(arm_len, th[0], 1.6, bone, 0.3)], shoulder=SHOULDER,
           base_y=-22, amp=(0, 16, 3), move=(0, -10, 0), phase=0.0)
    b.bone("fore_l", "arm_l", 1.0, a_fore, fore_len, [b.bone_rect(fore_len, th[1], 1.6, bone, 0.3)],
           amp=(0, 10, 3), phase=0.55)
    b.bone("horn_l", "arm_l", 1.0, a_arm - 40, 6 if ribs else 5, [b.claw_rect(0, 6 if ribs else 5, 3, 1.8, CLAW, 0.3)],
           phase=0.1)
    for n, t in enumerate(spikes):
        b.bone(f"spike{n}_l", "fore_l", t, a_fore - 80, 5, [b.claw_rect(0, 5, 3, 1.8, CLAW)], phase=0.55)
    b.bone("thumb_l", "fore_l", 1.0, a_fore - 62, 6, [b.claw_rect(0, 6, 3, 1.8, CLAW, 0.45)], phase=0.55)

    angles = [f[0] for f in fingers]
    lengths = [f[1] for f in fingers]
    for n, (ang, L) in enumerate(fingers):
        last = n == len(fingers) - 1
        if last:
            delta, partner = anchor[0] - ang, anchor[1]
        else:
            delta, partner = angles[n + 1] - ang, lengths[n + 1]
        rects = [b.bone_rect(L, th[2], 1.5, bone, 0.5),
                 b.claw_rect(L - 0.5, 4, 2, 1.5, CLAW, 0.35),
                 b.membrane(None, L, partner, delta, mem, bite, jag, holes if not last else holes + 1, ribs, seed + n)]
        b.bone(f"fin{n}_l", "fore_l", 1.0, ang, L, rects, amp=(0, 5 + n * 1.2, 4), phase=0.8 + 0.12 * n)
    return finish(b, id, name, rarity, desc, color, scale, speed, card)


def demon():
    return bat_wing("wings_demon", "Dämonenflügel", "epic", "Zerfetzte Fledermausflügel mit Krallen und Löchern.",
                    "#c4161c", 0.68, 2.1, 1.9, 11, 15, -66, -20,
                    [(-50, 24), (-10, 27), (28, 23), (66, 16)], (124, 26),
                    MEM_DEMON, BONE_DEMON, (3, 2, 2), 3.2, 1.3, 2, False, [], 10)


def dragon():
    return bat_wing("wings_dragon", "Drachenflügel", "legendary", "Breite Schuppenflügel mit Stachelkamm.",
                    "#3aa655", 0.78, 1.7, 1.5, 12, 16, -62, -26,
                    [(-52, 24), (-24, 31), (4, 30), (32, 26), (60, 19)], (124, 30),
                    MEM_DRAGON, BONE_DRAGON, (4, 3, 2), 3.2, 0.0, 0, True, [0.25, 0.55, 0.85], 20)


def cape():
    W, H = 30, 48
    atlas = Atlas(W)

    def paint(i, j, w, h):
        t = j / (h - 1)
        c = lerp((24, 34, 58), (8, 10, 18), t)
        if i < 1 or i >= w - 1:
            c = lerp(c, (62, 224, 160), 0.55)
        cx, cy = w / 2 - 0.5, 15.0
        r = math.hypot(i - cx, j - cy)
        if 7.0 <= r <= 9.0 or (abs(i - cx) < 0.9 and r < 9) or (abs(j - cy) < 0.9 and r < 9):
            c = (62, 224, 160)
        return c + (255,)
    uv = atlas.alloc(W, H, paint)
    bones = []
    for i in range(8):
        rect = Rect(-5, 0, 5, 2.15, 0, 0.6, (uv[0], uv[1] + 6 * i, uv[2], uv[1] + 6 * i + 6))
        bones.append(Bone(f"row{i}", f"row{i-1}" if i else None, (0, 0, 2.8) if i == 0 else (0, 2, 0),
                          0, 1.6, 0, (2.0, 0, 0.8), (5.0, 0, 0), 1.8, -i * 0.55, [rect]))
    return Design("cape_atlas", "Atlas-Cape", "cape", "common", "Wehendes Cape mit Atlas-Globus.",
                  "body", atlas, bones, "#ffffff", 1.0, True)


DESIGNS = [angel, demon, dragon, cape]
QUAD_BUDGET = 6000


def panel_label(img, x, title, sub, font, small):
    d = ImageDraw.Draw(img)
    d.text((x + 22, 18), title, fill=(236, 237, 238, 255), font=font)
    d.text((x + 22, 58), sub, fill=(154, 160, 169, 255), font=small)


def main():
    for d in ("models", "textures", "previews"):
        os.makedirs(d, exist_ok=True)
    entries = [{
        "id": "halo", "name": "Halo", "slot": "hat", "rarity": "rare",
        "description": "Goldener Ring, der leicht über dem Kopf schwebt.",
        "model": "models/halo.json", "texture": "textures/halo.png", "preview": "previews/halo.png",
    }]
    designs = [fn() for fn in DESIGNS]
    for d in designs:
        d.atlas.image().save(f"textures/{d.id}.png")
        json.dump(d.model(), open(f"models/{d.id}.json", "w"), separators=(",", ":"))
        q = count_quads(d)
        print(f"{d.id}: {len(d.bones)} bones, {q} quads, texture {d.atlas.w}x{d.atlas.h}")
        assert q <= QUAD_BUDGET, f"{d.id} exceeds the quad budget ({q} > {QUAD_BUDGET})"
        tint = hex_rgb(d.default_color) if d.default_color else None
        if d.slot == "cape":
            card = Image.new("RGBA", (128, 128), (22, 24, 29, 255))
            card.alpha_composite(d.atlas.image().resize((80, 128), Image.NEAREST), (24, 0))
        else:
            card = render(d, 0.0, 0.0, (128, 128), d.card_scale, 0.0, tint=tint, cy_frac=0.42)
        card.save(f"previews/{d.id}.png")
        entries.append({
            "id": d.id, "name": d.name, "slot": d.slot, "rarity": d.rarity, "description": d.description,
            "model": f"models/{d.id}.json", "texture": f"textures/{d.id}.png", "preview": f"previews/{d.id}.png",
            "default_color": d.default_color,
        })
    json.dump({"format": 1, "cosmetics": entries}, open("catalog.json", "w"), indent=2, ensure_ascii=False)

    wings = [d for d in designs if d.slot == "wings"]
    try:
        font = ImageFont.truetype("DejaVuSans-Bold.ttf", 28)
        small = ImageFont.truetype("DejaVuSans.ttf", 18)
    except OSError:
        font = small = ImageFont.load_default()
    scales = {"wings_angel": 6.0, "wings_demon": 7.2, "wings_dragon": 5.6}

    # static sheet: rear view and three-quarter view, rest pose
    pw, ph = 520, 470
    sheet = Image.new("RGBA", (pw * 3, 100 + ph * 2), (16, 18, 22, 255))
    for n, d in enumerate(wings):
        tint = hex_rgb(d.default_color)
        for row, phi in enumerate((0.0, math.radians(38))):
            panel = render(d, 0.0, 0.0, (pw, ph), scales[d.id], phi, tint=tint, cy_frac=0.42)
            sheet.alpha_composite(panel, (n * pw, 100 + row * ph))
        panel_label(sheet, n * pw, d.name, d.description, font, small)
    sheet.convert("RGB").save("previews/design-sheet.png")

    # animation: one full flap cycle, rear and three-quarter view; idle (breathing) and walking (full flap)
    gw, gh, frames = 330, 330, 36
    for name, move in (("wings-animated.gif", 0.0), ("wings-walking.gif", 1.0)):
        gif = []
        for k in range(frames):
            t = 2 * math.pi * k / frames
            frame = Image.new("RGBA", (gw * 3, gh * 2), (16, 18, 22, 255))
            for n, d in enumerate(wings):
                tint = hex_rgb(d.default_color)
                for row, phi in enumerate((0.0, math.radians(38))):
                    frame.alpha_composite(render(d, t / d.speed, move, (gw, gh), scales[d.id] * gw / pw, phi, tint=tint,
                                                 cy_frac=0.42), (n * gw, row * gh))
            gif.append(frame.convert("RGB").quantize(colors=96, method=Image.MEDIANCUT, dither=Image.NONE))
        gif[0].save(f"previews/{name}", save_all=True, append_images=gif[1:], duration=45, loop=0, optimize=True)


if __name__ == "__main__":
    main()
