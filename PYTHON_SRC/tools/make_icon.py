# -*- coding: utf-8 -*-
"""生成手绘风格的抖音音符图标（纯标准库，无需 Pillow）。

风格：抖音音符涂鸦 —— 米白色主体 + 青/红重影（抖音经典色差效果），
深色描边带随机抖动的双笔触，模拟铅笔手绘感。背景透明。

运行：
    python tools/make_icon.py

输出：
    WIN_C/src/app.ico          多尺寸 ICO（256/48/32/16），由 WIN_C/src/app.rc 嵌进 C++ 版 exe
    rust/app.ico               同一份图标，由 rust/app.rc 嵌进 Rust 版 exe
    WIN_C/build/icon_preview.png   预览图（方便人工检查，不参与构建）
"""
import math
import os
import random
import struct
import zlib

S = 1024                       # 超采样画布（4x），最后缩到各尺寸
random.seed(7)                 # 固定种子，每次生成一致

CYAN = (37, 244, 238)          # 抖音青
RED = (254, 44, 85)            # 抖音红
MAIN = (246, 244, 238)         # 米白主体（手绘感）
PENCIL = (58, 56, 66)          # 铅笔描边

def _find_project_root(start):
    """向上最多找 4 层，定位含 danmu/ 的项目根目录（本文件位于 PYTHON_SRC/tools/ 下）。"""
    d = start
    for _ in range(4):
        if os.path.isdir(os.path.join(d, "danmu")):
            return d
        nd = os.path.dirname(d)
        if nd == d:
            break
        d = nd
    return start


ROOT = _find_project_root(os.path.dirname(os.path.abspath(__file__)))
OUT_ICOS = [os.path.join(ROOT, "WIN_C", "src", "app.ico"),
            os.path.join(ROOT, "rust", "app.ico")]
OUT_PNG = os.path.join(ROOT, "WIN_C", "build", "icon_preview.png")

# ---- 音符形状（1024 画布坐标）：椭圆符头 + 符杆 + 符尾 ----
HEAD = dict(cx=396, cy=664, rx=152, ry=110, rot=math.radians(-22))
STEM = [(498, 268), (574, 258), (562, 664), (486, 672)]
FLAG = [(500, 268), (574, 258), (762, 296), (806, 372), (762, 452),
        (716, 400), (648, 372), (566, 352), (502, 336)]


def wobble_poly(pts, amp=9, steps=6):
    """多边形边细分并加随机抖动，模拟手绘线条。"""
    out = []
    n = len(pts)
    for i in range(n):
        x1, y1 = pts[i]
        x2, y2 = pts[(i + 1) % n]
        for k in range(steps):
            t = k / steps
            out.append((x1 + (x2 - x1) * t + random.uniform(-amp, amp),
                        y1 + (y2 - y1) * t + random.uniform(-amp, amp)))
    return out


def point_in_poly(x, y, poly):
    inside = False
    j = len(poly) - 1
    for i in range(len(poly)):
        xi, yi = poly[i]
        xj, yj = poly[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            inside = not inside
        j = i
    return inside


def ellipse_test(x, y):
    dx = x - HEAD["cx"]
    dy = y - HEAD["cy"]
    c = math.cos(-HEAD["rot"])
    s = math.sin(-HEAD["rot"])
    u = dx * c - dy * s
    v = dx * s + dy * c
    th = math.atan2(v / HEAD["ry"], u / HEAD["rx"])
    # 半径随角度低频起伏，手绘椭圆感
    w = 1.0 + 0.045 * math.sin(3 * th + 1.3) + 0.03 * math.sin(7 * th + 0.5)
    return (u / HEAD["rx"]) ** 2 + (v / HEAD["ry"]) ** 2 <= w * w


def rasterize():
    """音符填充掩码（1 = 实心）。"""
    mask = bytearray(S * S)
    stem_poly = wobble_poly(STEM)
    flag_poly = wobble_poly(FLAG)
    for y in range(S):
        base = y * S
        py = y + 0.5
        for x in range(S):
            px = x + 0.5
            if ellipse_test(px, py) or point_in_poly(px, py, stem_poly) \
                    or point_in_poly(px, py, flag_poly):
                mask[base + x] = 1
    return mask


def outline_mask(mask, width, ox=0, oy=0):
    """取掩码边界并膨胀 width/2，得到描边掩码；可整体偏移模拟第二笔触。"""
    boundary = []
    for y in range(S):
        base = y * S
        for x in range(S):
            if not mask[base + x]:
                continue
            if x == 0 or y == 0 or x == S - 1 or y == S - 1 \
                    or not mask[base + x - 1] or not mask[base + x + 1] \
                    or not mask[base - S + x] or not mask[base + S + x]:
                boundary.append((x, y))
    r = width // 2
    out = bytearray(S * S)
    span = range(-r, r + 1)
    rr = r * r
    for x, y in boundary:
        cx = x + ox
        cy = y + oy
        for dy in span:
            ny = cy + dy
            if ny < 0 or ny >= S:
                continue
            row = ny * S
            dy2 = dy * dy
            for dx in span:
                if dy2 + dx * dx > rr:
                    continue
                nx = cx + dx
                if 0 <= nx < S:
                    out[row + nx] = 1
    return out


def paint(img, mask, color, dx=0, dy=0, alpha=255):
    """把掩码区域涂上颜色（alpha<255 时做 over 混合）。"""
    r, g, b = color
    for y in range(S):
        sy = y - dy
        if sy < 0 or sy >= S:
            continue
        for x in range(S):
            sx = x - dx
            if sx < 0 or sx >= S or not mask[sy * S + sx]:
                continue
            i = (y * S + x) * 4
            if alpha >= 255 or img[i + 3] == 0:
                img[i] = r
                img[i + 1] = g
                img[i + 2] = b
                img[i + 3] = alpha if img[i + 3] == 0 else max(img[i + 3], alpha)
            else:
                a = alpha / 255.0
                ia = 1.0 - a
                img[i] = int(r * a + img[i] * ia)
                img[i + 1] = int(g * a + img[i + 1] * ia)
                img[i + 2] = int(b * a + img[i + 2] * ia)


def render():
    mask = rasterize()
    edge1 = outline_mask(mask, 14)                    # 主描边
    edge2 = outline_mask(mask, 7, ox=5, oy=-4)        # 偏移第二笔触，铅笔感
    img = bytearray(S * S * 4)
    paint(img, mask, CYAN, dx=-18, dy=18)             # 青重影（左下）
    paint(img, mask, RED, dx=18, dy=-18)              # 红重影（右上）
    paint(img, mask, MAIN)                            # 米白主体
    paint(img, edge1, PENCIL, alpha=210)              # 铅笔描边
    paint(img, edge2, PENCIL, alpha=110)
    return img


def box_resize_4x(img):
    """1024 → 256，4x4 盒式平均（预乘 alpha 保证边缘正确）。"""
    d = 256
    out = bytearray(d * d * 4)
    for y in range(d):
        for x in range(d):
            pr = pg = pb = pa = 0
            for sy in range(y * 4, y * 4 + 4):
                for sx in range(x * 4, x * 4 + 4):
                    i = (sy * S + sx) * 4
                    a = img[i + 3]
                    pr += img[i] * a
                    pg += img[i + 1] * a
                    pb += img[i + 2] * a
                    pa += a
            o = (y * d + x) * 4
            if pa:
                out[o] = pr // pa
                out[o + 1] = pg // pa
                out[o + 2] = pb // pa
                out[o + 3] = pa // 16
    return out


def bilinear_resize(src, sw, d):
    """256 → 小尺寸，双线性（预乘 alpha）。"""
    out = bytearray(d * d * 4)
    scale = sw / d
    for y in range(d):
        fy = (y + 0.5) * scale - 0.5
        y0 = max(0, int(math.floor(fy)))
        y1 = min(sw - 1, y0 + 1)
        wy = fy - y0
        for x in range(d):
            fx = (x + 0.5) * scale - 0.5
            x0 = max(0, int(math.floor(fx)))
            x1 = min(sw - 1, x0 + 1)
            wx = fx - x0
            pr = pg = pb = pa = 0.0
            for yy, wyy in ((y0, 1 - wy), (y1, wy)):
                for xx, wxx in ((x0, 1 - wx), (x1, wx)):
                    w = wyy * wxx
                    i = (yy * sw + xx) * 4
                    a = src[i + 3]
                    pr += src[i] * a * w
                    pg += src[i + 1] * a * w
                    pb += src[i + 2] * a * w
                    pa += a * w
            o = (y * d + x) * 4
            if pa > 0.5:
                out[o] = int(pr / pa + 0.5)
                out[o + 1] = int(pg / pa + 0.5)
                out[o + 2] = int(pb / pa + 0.5)
                out[o + 3] = int(pa + 0.5)
    return out


def dib(rgba, size):
    """ICO 内嵌的 BMP（DIB）：BITMAPINFOHEADER + 自底向上 BGRA + AND 掩码。"""
    mask_row = ((size + 31) // 32) * 4
    pixels = bytearray()
    for y in range(size - 1, -1, -1):
        for x in range(size):
            i = (y * size + x) * 4
            pixels += bytes((rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]))
    and_mask = bytes(mask_row * size)
    header = struct.pack("<IIIHHIIIIII", 40, size, size * 2, 1, 32, 0,
                         len(pixels) + len(and_mask), 0, 0, 0, 0)
    return header + bytes(pixels) + and_mask


def write_ico(images, path):
    """images: {size: rgba_bytes} → ICO 文件。"""
    entries = []
    data = bytearray()
    offset = 6 + 16 * len(images)
    for size in sorted(images, reverse=True):
        blob = dib(images[size], size)
        entries.append(struct.pack("<BBBBHHII",
                                   0 if size >= 256 else size,
                                   0 if size >= 256 else size,
                                   0, 0, 1, 32, len(blob), offset))
        data += blob
        offset += len(blob)
    header = struct.pack("<HHH", 0, 1, len(images))
    with open(path, "wb") as f:
        f.write(header + b"".join(entries) + bytes(data))


def write_png(rgba, size, path):
    """预览 PNG（zlib 压缩，无第三方库）。"""
    raw = bytearray()
    for y in range(size):
        raw.append(0)   # filter: none
        raw += rgba[y * size * 4:(y + 1) * size * 4]
    def chunk(tag, payload):
        c = struct.pack(">I", len(payload)) + tag + payload
        return c + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", ihdr))
        f.write(chunk(b"IDAT", zlib.compress(bytes(raw), 9)))
        f.write(chunk(b"IEND", b""))


def main():
    img = render()
    rgba256 = box_resize_4x(img)
    images = {256: rgba256}
    for size in (48, 32, 16):
        images[size] = bilinear_resize(rgba256, 256, size)
    for out_ico in OUT_ICOS:
        os.makedirs(os.path.dirname(out_ico), exist_ok=True)
        write_ico(images, out_ico)
        print("OK 图标已生成: %s（%d 字节）" % (out_ico, os.path.getsize(out_ico)))
    os.makedirs(os.path.dirname(OUT_PNG), exist_ok=True)
    write_png(rgba256, 256, OUT_PNG)
    print("OK 预览图: %s" % OUT_PNG)


if __name__ == "__main__":
    main()
