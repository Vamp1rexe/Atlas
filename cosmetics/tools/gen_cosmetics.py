#!/usr/bin/env python3
"""Generates the built-in Atlas cosmetics: models, textures, previews and catalog.json.

Run from the repo root:  python3 cosmetics/tools/gen_cosmetics.py
Needs Pillow. Add a new cosmetic by writing a function that returns a `Design`
and listing it in DESIGNS at the bottom.
"""
import json
import math
import os
from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
os.chdir(ROOT)


# ---------------------------------------------------------------- texture atlas
class Atlas:
    def __init__(self, width):
        self.w, self.h, self.x, self.y, self.rowh = width, 0, 0, 0, 0
        self.items = []

    def alloc(self, w, h, painter):
        if self.x + w > self.w:
            self.x, self.y, self.rowh = 0, self.y + self.rowh + 1, 0
        pos = (self.x, self.y)
        self.x += w + 1
        self.rowh = max(self.rowh, h)
        self.h = max(self.h, self.y + h)
        self.items.append((pos, w, h, painter))
        return (pos[0], pos[1], pos[0] + w, pos[1] + h)

    def image(self):
        img = Image.new("RGBA", (self.w, max(self.h, 1)), (0, 0, 0, 0))
        px = img.load()
        for (x0, y0), w, h, painter in self.items:
            for j in range(h):
                for i in range(w):
                    px[x0 + i, y0 + j] = painter(i, j, w, h)
        return img


# ---------------------------------------------------------------- bones
class Rect:
    """Flat slab in the bone's local frame (y down), z relative to the pivot."""

    def __init__(self, x0, y0, x1, y1, z0, z1, uv):
        self.x0, self.y0, self.x1, self.y1, self.z0, self.z1, self.uv = x0, y0, x1, y1, z0, z1, uv


class Bone:
    def __init__(self, name, parent, rel, rot_z=0.0, base=(0, 0), amp=(0, 0, 0), move=(0, 0, 0),
                 speed=2.0, phase=0.0, rects=None, base_x=0.0):
        self.name, self.parent, self.rel = name, parent, rel  # rel: pivot offset to parent (x, y, z)
        self.rot_z = rot_z                                      # static in-plane angle
        self.base_x, self.base_y = base_x, base[0] if base else 0
        self.amp, self.move, self.speed, self.phase = amp, move, speed, phase
        self.rects = rects or []


def mirror(bones, suffix="_r"):
    """Mirror a left wing into a right wing (x -> -x, uv flipped)."""
    out = []
    for b in bones:
        m = Bone(b.name.replace("_l", suffix) if b.name.endswith("_l") else b.name + suffix,
                 (b.parent.replace("_l", suffix) if b.parent and b.parent.endswith("_l") else (b.parent + suffix if b.parent else None)),
                 (-b.rel[0], b.rel[1], b.rel[2]), -b.rot_z, (-b.base_y,), (b.amp[0], -b.amp[1], -b.amp[2]),
                 (b.move[0], -b.move[1], -b.move[2]), b.speed, b.phase, None, b.base_x)
        m.rects = [Rect(-r.x1, r.y0, -r.x0, r.y1, r.z0, r.z1, (r.uv[2], r.uv[1], r.uv[0], r.uv[3])) for r in b.rects]
        out.append(m)
    return out


class Design:
    def __init__(self, id, name, slot, rarity, description, attach, atlas, bones, root_pivot,
                 default_color="", scale=1.0, art_from_behind=False):
        self.id, self.name, self.slot, self.rarity, self.description = id, name, slot, rarity, description
        self.attach, self.atlas, self.bones, self.root_pivot = attach, atlas, bones, root_pivot
        self.default_color, self.scale, self.art_from_behind = default_color, scale, art_from_behind

    # -- model json
    def pivots(self):
        piv = {}
        for b in self.bones:
            pp = piv[b.parent] if b.parent else self.root_pivot
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
                # seen from behind the south face runs toward -x: wings are drawn along +x, capes as seen
                faces = {"south": list(r.uv) if self.art_from_behind else rev,
                         "north": rev if self.art_from_behind else list(r.uv)}
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

    # -- 2d preview of the rest pose, seen from behind
    def render(self, size, scale, bg=(22, 24, 29, 255), tint=None, body=True):
        img = Image.new("RGBA", (size[0], size[1]), bg)
        S = scale
        cx, cy = size[0] / 2, size[1] * 0.36
        d = ImageDraw.Draw(img)
        if body:  # player silhouette: head, torso, arms, legs
            def box(x0, y0, x1, y1, col):
                d.rectangle([cx + x0 * S, cy + y0 * S, cx + x1 * S, cy + y1 * S], fill=col)
            box(-4, -8, 4, 0, (82, 88, 99, 255)); box(-4, 0, 4, 12, (70, 76, 88, 255))
            box(-8, 0, -4, 12, (62, 68, 79, 255)); box(4, 0, 8, 12, (62, 68, 79, 255))
            box(-4, 12, 0, 24, (54, 59, 69, 255)); box(0, 12, 4, 24, (54, 59, 69, 255))
        tex = self.atlas.image()
        if tint:
            px = tex.load()
            for j in range(tex.height):
                for i in range(tex.width):
                    r, g, b, a = px[i, j]
                    px[i, j] = (r * tint[0] // 255, g * tint[1] // 255, b * tint[2] // 255, a)
        world, angle = {}, {}
        draws = []
        piv = self.pivots()
        for b in self.bones:
            if b.parent:
                pw, pa = world[b.parent], angle[b.parent]
                c, s = math.cos(math.radians(pa)), math.sin(math.radians(pa))
                world[b.name] = (pw[0] + b.rel[0] * c - b.rel[1] * s, pw[1] + b.rel[0] * s + b.rel[1] * c)
                angle[b.name] = pa + b.rot_z
            else:
                world[b.name] = (self.root_pivot[0] + b.rel[0], self.root_pivot[1] + b.rel[1])
                angle[b.name] = b.rot_z
            for r in b.rects:
                draws.append((piv[b.name][2] + r.z0, b, r))
        for _, b, r in sorted(draws, key=lambda t: t[0]):
            u0, v0, u1, v1 = r.uv
            crop = tex.crop((min(u0, u1), min(v0, v1), max(u0, u1), max(v0, v1)))
            if u1 < u0:
                crop = crop.transpose(Image.FLIP_LEFT_RIGHT)
            w, h = (r.x1 - r.x0), (r.y1 - r.y0)
            crop = crop.resize((max(1, round(w * S)), max(1, round(h * S))), Image.NEAREST)
            a = angle[b.name]
            rot = crop.rotate(-a, expand=True, resample=Image.BICUBIC)
            c, s = math.cos(math.radians(a)), math.sin(math.radians(a))
            lx, ly = (r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2
            wx = world[b.name][0] + lx * c - ly * s
            wy = world[b.name][1] + lx * s + ly * c
            img.alpha_composite(rot, (round(cx + wx * S - rot.width / 2), round(cy + wy * S - rot.height / 2)))
        return img


# ---------------------------------------------------------------- designs
def lerp(a, b, t):
    return tuple(round(a[k] + (b[k] - a[k]) * t) for k in range(3))


def angel():
    atlas = Atlas(192)

    def feather(L, w):
        base, hi, lo, vein, tip = (246, 247, 251), (255, 255, 255), (205, 213, 230), (255, 252, 240), (190, 202, 226)

        def paint(i, j, W, H):
            taper = i - (L - 4)
            if taper > 0 and (j < taper * (w / 8.0) or j >= w - taper * (w / 8.0)):
                return (0, 0, 0, 0)
            col = base
            if j == 0: col = hi
            elif j == w - 1: col = lo
            elif j == w // 2: col = vein
            if i > L - 7: col = lerp(col, tip, (i - (L - 7)) / 7.0)
            return col + (255,)
        return paint

    arm = 16
    bones = [Bone("wing_l", None, (2.5, 3.0, 2.3), -55, (-22,), (0, 18, 4), (0, 10, 0), 2.4, 0.0,
                  [Rect(0, -1.2, arm + 1, 1.2, 0, 1.0, atlas.alloc(arm + 1, 2, lambda i, j, W, H: (240, 243, 250, 255)))])]
    k = 0

    def add(positions, angles, lengths, w, z):
        nonlocal k
        for sx, a, L in zip(positions, angles, lengths):
            uv = atlas.alloc(L, w, feather(L, w))
            bones.append(Bone(f"f{k}_l", "wing_l", (sx, 0, 0), a, (0,), (0, 2.5 + L / 14.0, 0), (0, 0, 0),
                              2.4, 0.22 * k, [Rect(0.5, -w / 2, 0.5 + L, w / 2, z, z + 0.5, uv)]))
            k += 1
    sec_s = [3, 5.5, 8, 10.5, 13, 15]
    sec_a = [100, 98, 95, 92, 88, 85]
    sec_l = [8, 11, 14, 17, 19, 20]
    add(sec_s, sec_a, sec_l, 5, 0.0)                                       # secondaries (back layer)
    add([arm] * 5, [10, 28, 46, 64, 82], [22, 27, 29, 26, 21], 5, 0.7)     # primaries
    add(sec_s, [a - 8 for a in sec_a], [round(l * 0.55) + 1 for l in sec_l], 5, 1.4)  # coverts (front)
    bones += mirror(bones)
    return Design("wings_angel", "Engelsflügel", "wings", "epic",
                  "Weiße Federflügel, die ruhig schlagen.", "body", atlas, bones, (0, 0, 0), "#ffffff", 1.0)


def bat_wing(id, name, rarity, desc, arm, fore, fingers, mem_w, color, flap_amp, speed, spikes, scale, scalloped):
    atlas = Atlas(192)
    bone_col, bone_hi = (122, 122, 122), (168, 168, 168)

    def bone_paint(i, j, W, H):
        c = bone_hi if j == 0 else (bone_col if j < H - 1 else (88, 88, 88))
        return c + (255,)

    def membrane(L, w):
        def paint(i, j, W, H):
            if scalloped and j >= w - 2 and (i % 6) > 2:
                return (0, 0, 0, 0)
            t = j / max(1, w - 1)
            c = lerp((222, 222, 222), (178, 178, 178), t)
            if i % 5 == 0: c = lerp(c, (120, 120, 120), 0.55)       # veins
            if j == w - 1: c = lerp(c, (110, 110, 110), 0.6)         # trailing edge
            return c + (255,)
        return paint

    bones = []
    arm_uv = atlas.alloc(arm, 3, bone_paint)
    sail_uv = atlas.alloc(arm - 2, 10, membrane(arm - 2, 10))
    rects = [Rect(0, -1.5, arm, 1.5, 0, 1.4, arm_uv), Rect(2, 1.5, arm, 11.5, 0.45, 0.95, sail_uv)]
    for n in range(spikes):
        sx = 3 + n * (arm - 5) / max(1, spikes - 1) if spikes > 1 else 4
        rects.append(Rect(sx, -3.6, sx + 1.6, -1.5, 0.2, 1.2, atlas.alloc(2, 2, bone_paint)))
    bones.append(Bone("arm_l", None, (2.5, 3.0, 2.3), -50, (-20,), (0, flap_amp, 3), (0, 10, 0), speed, 0.0, rects))
    bones.append(Bone("fore_l", "arm_l", (arm, 0, 0), 65, (0,), (0, flap_amp * 0.45, 0), (0, 4, 0), speed, 0.5,
                      [Rect(0, -1.2, fore, 1.2, 0.1, 1.2, atlas.alloc(fore, 3, bone_paint))]))
    for n, (ang, L) in enumerate(fingers):
        mw = mem_w - (2 if n == len(fingers) - 1 else 0)
        rects = [Rect(0, -0.7, L, 0.7, 0.1, 1.1, atlas.alloc(L, 2, bone_paint)),
                 Rect(2, 0.7, L - 1, 0.7 + mw, 0.35, 0.85, atlas.alloc(L - 3, mw, membrane(L - 3, mw))),
                 Rect(L, -0.6, L + 2.6, 0.6, 0.1, 1.1, atlas.alloc(3, 1, lambda i, j, W, H: (60, 60, 60, 255)))]
        bones.append(Bone(f"fin{n}_l", "fore_l", (fore, 0, 0), ang, (0,), (0, flap_amp * 0.35 + n * 1.5, 0), (0, 3, 0),
                          speed, 0.9 + n * 0.3, rects))
    bones += mirror(bones)
    return Design(id, name, "wings", rarity, desc, "body", atlas, bones, (0, 0, 0), color, scale)


def demon():
    return bat_wing("wings_demon", "Dämonenflügel", "epic", "Zerfetzte Fledermausflügel mit Krallen.",
                    14, 14, [(-40, 24), (-5, 27), (38, 20)], 13, "#c4161c", 16, 2.1, 0, 1.0, True)


def dragon():
    return bat_wing("wings_dragon", "Drachenflügel", "legendary", "Riesige Schuppenflügel mit Stacheln.",
                    18, 18, [(-48, 28), (-22, 34), (6, 32), (34, 24)], 16, "#3aa655", 20, 1.7, 3, 0.85, False)


def cape():
    W, H = 30, 48
    atlas = Atlas(W)

    def paint(i, j, w, h):
        t = j / (h - 1)
        c = lerp((24, 34, 58), (8, 10, 18), t)
        if i < 1 or i >= w - 1: c = lerp(c, (62, 224, 160), 0.55)
        cx, cy = w / 2 - 0.5, 15.0
        r = math.hypot(i - cx, (j - cy))
        if 7.0 <= r <= 9.0 or (abs(i - cx) < 0.9 and r < 9) or (abs(j - cy) < 0.9 and r < 9):
            c = (62, 224, 160)
        return c + (255,)
    uv = atlas.alloc(W, H, paint)
    bones = []
    for i in range(8):
        rect = Rect(-5, 0, 5, 2.15, 0, 0.6, (uv[0], uv[1] + 6 * i, uv[2], uv[1] + 6 * i + 6))
        if i == 0:
            bones.append(Bone("row0", None, (0, 0, 2.8), 0, (0,), (0, 0, 0), (0, 0, 0), 1.8, 0.0, [rect], 1.6))
        else:
            bones.append(Bone(f"row{i}", f"row{i-1}", (0, 2, 0), 0, (0,), (0, 0, 0), (0, 0, 0), 1.8, 0.0, [rect], 1.6))
        b = bones[-1]
        b.amp, b.move, b.phase = (2.0, 0, 0.8), (5.0, 0, 0), -i * 0.55
    return Design("cape_atlas", "Atlas-Cape", "cape", "common", "Wehendes Cape mit Atlas-Globus.",
                  "body", atlas, bones, (0, 0, 0), "#ffffff", 1.0, art_from_behind=True)


DESIGNS = [angel, demon, dragon, cape]


def hex_rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4)) if len(h) == 6 else (255, 255, 255)


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
        tint = hex_rgb(d.default_color) if d.default_color else None
        if d.slot == "cape":
            prev = d.atlas.image().resize((120, 192), Image.NEAREST)
            card = Image.new("RGBA", (128, 128), (22, 24, 29, 255))
            card.alpha_composite(prev.resize((80, 128), Image.NEAREST), (24, 0))
            prev = card
        else:
            prev = d.render((128, 128), 1.45, tint=tint, body=True)
        prev.save(f"previews/{d.id}.png")
        entries.append({
            "id": d.id, "name": d.name, "slot": d.slot, "rarity": d.rarity, "description": d.description,
            "model": f"models/{d.id}.json", "texture": f"textures/{d.id}.png", "preview": f"previews/{d.id}.png",
            "default_color": d.default_color,
        })
    json.dump({"format": 1, "cosmetics": entries}, open("catalog.json", "w"), indent=2, ensure_ascii=False)

    # design sheet: the three wings, both sides, on a player silhouette
    sheet = Image.new("RGBA", (1560, 640), (16, 18, 22, 255))
    try:
        font = ImageFont.truetype("DejaVuSans-Bold.ttf", 30)
        small = ImageFont.truetype("DejaVuSans.ttf", 20)
    except OSError:
        font = small = ImageFont.load_default()
    d = ImageDraw.Draw(sheet)
    for n, design in enumerate([x for x in designs if x.slot == "wings"]):
        panel = design.render((520, 500), {"wings_angel": 6.0, "wings_demon": 4.6, "wings_dragon": 3.4}[design.id],
                              bg=(22, 24, 29, 255), tint=hex_rgb(design.default_color))
        sheet.alpha_composite(panel, (n * 520 + 0, 140))
        d.text((n * 520 + 24, 24), design.name, fill=(236, 237, 238, 255), font=font)
        d.text((n * 520 + 24, 70), design.description, fill=(154, 160, 169, 255), font=small)
        d.text((n * 520 + 24, 100), "Farbe und Größe im Launcher einstellbar", fill=(27, 217, 106, 255), font=small)
    sheet.convert("RGB").save("previews/design-sheet.png")


if __name__ == "__main__":
    main()
