#!/usr/bin/env python3
"""Turns Anisimova's Mahjong game pack (one Illustrator EPS sheet) into 42 transparent tile PNGs.

    python3 scripts/make-tiles.py "/path/to/Mahjong.eps" src/assets/tiles

Needs Ghostscript (`gs`) and poppler's `pdftocairo`; both are in Homebrew. See src/assets/tiles/NOTICE.md for the pack's terms.
The sheet paints each tile's shadow as opaque bitmaps with white baked in, so this reads every tile's outline from the PDF, renders the tile
and an exact silhouette mask from the same crop window, and keeps the tile opaque while turning the baked shadow into real transparency.
"""
import os, re, shutil, struct, subprocess, sys, tempfile, zlib

if len(sys.argv) != 3:
    sys.exit(__doc__)
EPS, OUT = os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2])
GS = shutil.which("gs") or "/opt/homebrew/bin/gs"
WORK = tempfile.mkdtemp(prefix="mjers-tiles-")
os.chdir(WORK)
subprocess.run([GS, "-q", "-dNOPAUSE", "-dBATCH", "-dSAFER", "-dEPSCrop", "-sDEVICE=pdfwrite", "-sOutputFile=pack.pdf", EPS], check=True)
_pdf = open("pack.pdf", "rb").read()
_start = re.search(rb"stream\r?\n", _pdf).end()
_text = zlib.decompress(_pdf[_start:_pdf.find(b"endstream", _start)]).decode("latin1")
open("content.txt", "w").write(_text)
PAGE_W, PAGE_H = 2867.5320, 2152.8137
S = 0.8
text = _text
line_re = re.compile(r"^([-\d.\se]+) (m|l|c)$")

# The outline of the tile whose box origin is (4141, 16730), second subpath of its clip.
REF = (4141.0, 16730.0)
outline = None
for mm in re.finditer(r"\nW n\n", text):
    start = text.rfind("\nq\n", 0, mm.start()) + 3
    lines = text[start:mm.start()].split("\n")
    if not all(line_re.match(l) for l in lines):
        continue
    xs = [float(v) for l in lines for v in line_re.match(l).group(1).split()[0::2]]
    ys = [float(v) for l in lines for v in line_re.match(l).group(1).split()[1::2]]
    if abs(min(xs) - REF[0]) < 2 and abs(min(ys) - REF[1]) < 2 and sum(1 for l in lines if l.endswith(" m")) == 2:
        ms = [i for i, l in enumerate(lines) if l.endswith(" m")]
        outline = lines[ms[1]:]
        break
assert outline, "reference outline not found"

cols9 = [1231, 4141, 7051, 9971, 12881, 15791, 18701, 21621, 24531]
bonus_x = [2691, 5601, 8511, 11421, 14331, 17251, 20161, 23071]
honor_names = ["honor-1", "honor-2", "honor-3", "honor-4", "honor-7", "honor-6", "honor-5"]  # sheet: E S W N Red Green White
bonus_names = ["season-1", "season-2", "season-3", "season-4", "flower-1", "flower-2", "flower-3", "flower-4"]
tiles = []
for kind, y in (("pin", 16730), ("sou", 12850), ("man", 8960)):
    for i, x in enumerate(cols9):
        tiles.append((f"{kind}-{i + 1}", x, y))
for i, x in enumerate(cols9[1:8]):
    tiles.append((honor_names[i], x, 5070))
for i, x in enumerate(bonus_x):
    tiles.append((bonus_names[i], x, 1180))
assert len(tiles) == 42

def shifted(dx, dy):
    out = []
    for l in outline:
        m = line_re.match(l)
        v = [float(t) for t in m.group(1).split()]
        v = [round(t + (dx if k % 2 == 0 else dy), 2) for k, t in enumerate(v)]
        out.append(" ".join(f"{t:g}" for t in v) + " " + m.group(2))
    return "\n".join(out)

def mask_pdf(path, dx, dy):
    content = f"q 0.1 0 0 0.1 0 0 cm 0 g\n{shifted(dx, dy)}\nf\nQ\n".encode()
    objs = [b"<< /Type /Catalog /Pages 2 0 R >>",
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Contents 4 0 R >>".encode(),
            b"<< /Length %d >>\nstream\n" % len(content) + content + b"endstream"]
    out = b"%PDF-1.4\n"; offs = []
    for i, o in enumerate(objs, 1):
        offs.append(len(out)); out += f"{i} 0 obj\n".encode() + o + b"\nendobj\n"
    x = len(out)
    out += f"xref\n0 {len(objs) + 1}\n0000000000 65535 f \n".encode()
    for o in offs: out += f"{o:010d} 00000 n \n".encode()
    out += f"trailer\n<< /Size {len(objs) + 1} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n".encode()
    open(path, "wb").write(out)

def load_png(path):
    d = open(path, "rb").read(); pos = 8; idat = b""
    while pos < len(d):
        n, t = struct.unpack(">I4s", d[pos:pos + 8]); body = d[pos + 8:pos + 8 + n]; pos += 12 + n
        if t == b"IHDR": w, h, bd, ct, _, _, il = struct.unpack(">IIBBBBB", body)
        if t == b"IDAT": idat += body
    assert bd == 8 and il == 0
    raw = zlib.decompress(idat); bpp = {0: 1, 2: 3, 6: 4}[ct]; stride = w * bpp
    rows = []; prev = bytearray(stride); p = 0
    for _ in range(h):
        f = raw[p]; line = bytearray(raw[p + 1:p + 1 + stride]); p += 1 + stride
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0; b = prev[i]; c = prev[i - bpp] if i >= bpp else 0
            if f == 1: line[i] = (line[i] + a) & 255
            elif f == 2: line[i] = (line[i] + b) & 255
            elif f == 3: line[i] = (line[i] + ((a + b) >> 1)) & 255
            elif f == 4:
                pa = abs(b - c); pb = abs(a - c); pc = abs(a + b - 2 * c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else (b if pb <= pc else c))) & 255
        rows.append(line); prev = line
    return w, h, bpp, rows

def save_rgba(path, w, h, rows):
    def chunk(t, b): return struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xffffffff)
    raw = b"".join(b"\x00" + bytes(r) for r in rows)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))

os.makedirs(OUT, exist_ok=True); os.makedirs("tmp", exist_ok=True)
for name, X, Y in tiles:
    mask_pdf("tmp/mask.pdf", X - REF[0], Y - REF[1])
    bx, by = X / 10, Y / 10                    # box lower-left in pt
    W, H = 289.5, 381.5
    px, py = round(bx * S), round((PAGE_H - (by + H)) * S)
    pw, ph = round(W * S), round(H * S)
    args = ["-png", "-singlefile", "-r", str(72 * S), "-x", str(px), "-y", str(py), "-W", str(pw), "-H", str(ph)]
    subprocess.run(["pdftocairo", *args, "pack.pdf", "tmp/color"], check=True)
    subprocess.run(["pdftocairo", *args, "-gray", "tmp/mask.pdf", "tmp/maskimg"], check=True)
    w, h, cb, crows = load_png("tmp/color.png"); mw, mh, mb, mrows = load_png("tmp/maskimg.png")
    assert (w, h) == (mw, mh)
    out = []
    for y in range(h):
        c = crows[y]; m = mrows[y]; row = bytearray(w * 4)
        for x in range(w):
            r, g, b = c[x * cb], c[x * cb + 1], c[x * cb + 2]
            cov = (255 - m[x * mb]) / 255.0                     # tile coverage 0..1
            sa = (255 - min(r, g, b)) / 255.0                    # baked-in shadow darkness
            a = cov + (1 - cov) * sa
            if a <= 0.002:
                continue
            k = x * 4
            row[k] = round(cov * r / a); row[k + 1] = round(cov * g / a); row[k + 2] = round(cov * b / a); row[k + 3] = round(a * 255)
        out.append(row)
    save_rgba(os.path.join(OUT, f"{name}.png"), w, h, out)
    print(name, w, h, flush=True)

shutil.rmtree(WORK, ignore_errors=True)
