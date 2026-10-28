#!/usr/bin/env python3
"""Convert the 24/32-bit XWD framebuffer written by `Xvfb -fbdir` to PNG.

Used by `build-aux/dev.sh screenshot`; avoids depending on ImageMagick.
"""
import struct
import sys
import zlib


def main(src, dst):
    data = open(src, "rb").read()
    fields = struct.unpack(">25I", data[:100])
    header_size, byte_order = fields[0], fields[7]
    width, height = fields[4], fields[5]
    bpp, bytes_per_line, ncolors = fields[11], fields[12], fields[19]
    if bpp != 32:
        sys.exit(f"unsupported bits per pixel: {bpp}")
    offset = header_size + ncolors * 12
    raw = bytearray()
    for y in range(height):
        row = data[offset + y * bytes_per_line : offset + y * bytes_per_line + width * 4]
        raw.append(0)
        for x in range(0, len(row), 4):
            b, g, r = (row[x], row[x + 1], row[x + 2]) if byte_order == 0 else (row[x + 3], row[x + 2], row[x + 1])
            raw += bytes((r, g, b))

    def chunk(kind, payload):
        out = struct.pack(">I", len(payload)) + kind + payload
        return out + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    open(dst, "wb").write(png)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
