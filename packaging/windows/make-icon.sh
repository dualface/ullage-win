#!/bin/bash
# Makes app-icon.ico from app-icon.svg: one PNG image for each size Windows
# asks for, from the 16x16 of a list to the 256x256 of a large icon view.
# Needs rsvg-convert (librsvg) and python3. Run it from any directory.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

sizes=(16 20 24 32 40 48 64 256)
for size in "${sizes[@]}"; do
    rsvg-convert -w "$size" -h "$size" "$here/app-icon.svg" -o "$work/$size.png"
done

# An ICO file is a directory of images; Windows Vista and later read PNG
# images in it at every size.
python3 - "$here/app-icon.ico" "$work" "${sizes[@]}" <<'PY'
import struct, sys
out, work, sizes = sys.argv[1], sys.argv[2], [int(s) for s in sys.argv[3:]]
images = [open(f"{work}/{size}.png", "rb").read() for size in sizes]
header = struct.pack("<HHH", 0, 1, len(images))
offset = len(header) + 16 * len(images)
entries = b""
for size, image in zip(sizes, images):
    # A width or height of 256 is written as 0.
    side = 0 if size == 256 else size
    entries += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(image), offset)
    offset += len(image)
with open(out, "wb") as f:
    f.write(header + entries + b"".join(images))
PY
echo "wrote $here/app-icon.ico"
