#!/usr/bin/env python3
"""
Creates placeholder icons for Tauri development builds.
Uses only the Python standard library (zlib + struct).

These are solid #22c55e (green) squares. Replace with real icons
by running: ./scripts/generate-icons.sh path/to/logo-1024.png
"""

import zlib
import struct
import os


def make_png(width: int, height: int, r: int, g: int, b: int) -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)  # color type 6 = RGBA
    raw = b""
    for _ in range(height):
        raw += b"\x00"
        raw += bytes([r, g, b, 0xFF] * width)
    idat_data = zlib.compress(raw)

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", idat_data)
        + chunk(b"IEND", b"")
    )


def make_ico(png_32: bytes) -> bytes:
    """Wrap a 32x32 PNG into a minimal .ICO file."""
    num_images = 1
    header = struct.pack("<HHH", 0, 1, num_images)
    image_offset = 6 + 16 * num_images
    entry = struct.pack(
        "<BBBBHHII",
        32, 32,     # width, height
        0,          # palette size
        0,          # reserved
        1,          # color planes
        32,         # bits per pixel
        len(png_32),
        image_offset,
    )
    return header + entry + png_32


R, G, B = 0x22, 0xC5, 0x5E

icons_dir = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")
os.makedirs(icons_dir, exist_ok=True)

sizes = {
    "32x32.png":      (32, 32),
    "128x128.png":    (128, 128),
    "128x128@2x.png": (256, 256),
    "icon.png":       (256, 256),
}

for filename, (w, h) in sizes.items():
    path = os.path.join(icons_dir, filename)
    data = make_png(w, h, R, G, B)
    with open(path, "wb") as f:
        f.write(data)
    print(f"  Created {filename} ({w}x{h})")

ico_path = os.path.join(icons_dir, "icon.ico")
with open(ico_path, "wb") as f:
    f.write(make_ico(make_png(32, 32, R, G, B)))
print("  Created icon.ico")

icns_path = os.path.join(icons_dir, "icon.icns")
png_512 = make_png(512, 512, R, G, B)
with open(icns_path, "wb") as f:
    f.write(b"icns")
    size = 8 + 4 + len(png_512)
    f.write(struct.pack(">I", size))
    f.write(b"ic09")
    f.write(struct.pack(">I", 8 + len(png_512)))
    f.write(png_512)
print("  Created icon.icns")

print("\nPlaceholder icons created. Replace with real icons before shipping.")
print("Run: ./scripts/generate-icons.sh path/to/logo-1024.png")
