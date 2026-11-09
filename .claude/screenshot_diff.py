"""Compare a client screenshot with an approved reference (#531). Standard library only.

  python -I client/.claude/screenshot_diff.py <shot.png> <reference.png> [--out diff.png] [--tolerance 8] [--max 0.5]
  python -I client/.claude/screenshot_diff.py --test

Prints the size of both images, the share of pixels whose largest channel difference is above
--tolerance (0-255, default 8: absorbs antialiasing and colour rounding), and the bounding box of
those pixels. --out writes a diff image: differing pixels in red over a dimmed copy of the
reference. Exit code 0 when the share is at most --max percent (default 0.5), 1 when it is above
or the sizes differ, 2 when a file cannot be read.

Reads 8-bit grayscale, RGB and RGBA PNGs (what Bevy's --screenshot writes), not interlaced.
"""
import argparse
import struct
import sys
import zlib


def read_png(path):
    """(width, height, rows) with rows a list of bytes objects of RGBA pixels."""
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path}: not a PNG")
    pos, idat, w = 8, b"", None
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        chunk = data[pos + 8:pos + 8 + n]
        pos += 12 + n
        if kind == b"IHDR":
            w, h, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", chunk)
            if depth != 8 or ctype not in (0, 2, 6) or interlace:
                raise ValueError(f"{path}: only 8-bit gray/RGB/RGBA, non-interlaced PNGs (got depth {depth}, type {ctype})")
        elif kind == b"IDAT":
            idat += chunk
        elif kind == b"IEND":
            break
    if w is None:
        raise ValueError(f"{path}: no IHDR")
    bpp = {0: 1, 2: 3, 6: 4}[ctype]
    raw, stride, rows, prev = zlib.decompress(idat), w * bpp, [], bytearray(w * bpp)
    for y in range(h):
        f = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if f == 1:
                line[i] = (line[i] + a) & 255
            elif f == 2:
                line[i] = (line[i] + b) & 255
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        prev = line
        if bpp == 4:
            rows.append(bytes(line))
        else:
            out = bytearray()
            for x in range(w):
                px = line[x * bpp:(x + 1) * bpp]
                out += (bytes(px) * 3 if bpp == 1 else bytes(px)) + b"\xff"
            rows.append(bytes(out))
    return w, h, rows


def write_png(path, w, h, rows):
    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
    raw = b"".join(b"\x00" + r for r in rows)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b"")
    open(path, "wb").write(png)


def compare(a, b, tolerance):
    """(differing pixel count, bounding box or None, diff rows) for two same-size images."""
    w, h, ra = a
    _, _, rb = b
    count, box, diff = 0, None, []
    for y in range(h):
        la, lb, out = ra[y], rb[y], bytearray()
        for x in range(0, w * 4, 4):
            d = max(abs(la[x + k] - lb[x + k]) for k in range(4))
            if d > tolerance:
                count += 1
                px = x // 4
                box = (px, y, px, y) if box is None else (min(box[0], px), min(box[1], y), max(box[2], px), max(box[3], y))
                out += b"\xff\x00\x00\xff"
            else:
                out += bytes(v // 3 for v in lb[x:x + 3]) + b"\xff"
        diff.append(bytes(out))
    return count, box, diff


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("shot")
    ap.add_argument("reference")
    ap.add_argument("--out")
    ap.add_argument("--tolerance", type=int, default=8)
    ap.add_argument("--max", type=float, default=0.5, help="largest share of differing pixels that passes, in percent")
    a = ap.parse_args(argv)
    try:
        shot, ref = read_png(a.shot), read_png(a.reference)
    except (OSError, ValueError, zlib.error) as e:
        print(f"error: {e}")
        return 2
    print(f"shot {shot[0]}x{shot[1]}, reference {ref[0]}x{ref[1]}")
    if shot[:2] != ref[:2]:
        print("FAIL: the sizes differ (use the same --window and --ratio as the reference)")
        return 1
    count, box, diff = compare(shot, ref, a.tolerance)
    share = 100 * count / (shot[0] * shot[1])
    print(f"differing pixels: {count} ({share:.3f}%), tolerance {a.tolerance}" + (f", in box x {box[0]}-{box[2]}, y {box[1]}-{box[3]}" if box else ""))
    if a.out:
        write_png(a.out, shot[0], shot[1], diff)
        print(f"diff image: {a.out}")
    ok = share <= a.max
    print(("PASS" if ok else "FAIL") + f": at most {a.max}% may differ")
    return 0 if ok else 1


def self_test():
    import os
    import tempfile
    w, h = 20, 10
    base = [b"".join(bytes([x * 10, y * 20, 100, 255]) for x in range(w)) for y in range(h)]
    changed = [bytearray(r) for r in base]
    for x in range(3, 6):
        changed[4][x * 4:x * 4 + 4] = b"\x00\xff\x00\xff"
    with tempfile.TemporaryDirectory() as d:
        pa, pb, pd = (os.path.join(d, n) for n in ("a.png", "b.png", "d.png"))
        write_png(pa, w, h, base)
        write_png(pb, w, h, [bytes(r) for r in changed])
        assert read_png(pa) == (w, h, base)
        assert main([pa, pa]) == 0
        assert main([pa, pb, "--out", pd, "--max", "1"]) == 1  # 3 of 200 pixels = 1.5%
        assert main([pa, pb, "--max", "2"]) == 0
        n, box, _ = compare(read_png(pa), read_png(pb), 8)
        assert (n, box) == (3, (3, 4, 5, 4)), (n, box)
        assert read_png(pd)[:2] == (w, h)
    print("screenshot_diff self-test passed")
    return 0


if __name__ == "__main__":
    sys.exit(self_test() if sys.argv[1:] == ["--test"] else main(sys.argv[1:]))
