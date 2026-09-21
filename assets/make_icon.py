from pathlib import Path

from PIL import Image, ImageDraw

BASE = (0x0D, 0x0F, 0x15, 255)
CONTRAST = (0xCC, 0xD1, 0xDB, 255)
ACCENT = (0x4B, 0xDC, 0x95, 255)
SIZES = [16, 24, 32, 48, 64, 128, 256]
WINDOW_ICON = 64
TERMINALS_FROM = 48

BOLT = [(0.60, 0.10), (0.28, 0.55), (0.47, 0.55), (0.40, 0.90), (0.72, 0.45), (0.53, 0.45)]
BRIDGED_BOLT = [(0.62, 0.12), (0.30, 0.55), (0.49, 0.55), (0.40, 0.88), (0.72, 0.45), (0.53, 0.45)]


def render(size: int, scale: int = 8, bridged: bool | None = None) -> Image.Image:
    s = size * scale
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    inset = max(1, round(s * 0.04))
    border = max(1, round(s * 0.035))
    d.rectangle([inset, inset, s - 1 - inset, s - 1 - inset], fill=CONTRAST)
    d.rectangle(
        [inset + border, inset + border, s - 1 - inset - border, s - 1 - inset - border],
        fill=BASE,
    )
    if bridged is None:
        bridged = size >= TERMINALS_FROM
    if bridged:
        w = max(1, round(s * 0.045))
        r = s * 0.055
        for (x0, x1, y), (cx, cy) in [
            ((0.12, 0.30, 0.30), (0.14, 0.30)),
            ((0.70, 0.88, 0.70), (0.86, 0.70)),
        ]:
            d.line([(x0 * s, y * s), (x1 * s, y * s)], fill=CONTRAST, width=w)
            d.ellipse([cx * s - r, cy * s - r, cx * s + r, cy * s + r], fill=CONTRAST)
    points = BRIDGED_BOLT if bridged else BOLT
    d.polygon([(x * s, y * s) for x, y in points], fill=ACCENT)
    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    out = Path(__file__).resolve().parent
    images = {size: render(size) for size in SIZES}
    largest = images[max(SIZES)]
    largest.save(
        out / "icon.ico",
        sizes=[(size, size) for size in SIZES],
        append_images=[images[size] for size in SIZES if size != max(SIZES)],
    )
    (out / "icon.rgba").write_bytes(render(WINDOW_ICON, bridged=False).tobytes())
    images[256].save(out / "icon.png")
    print("wrote icon.ico, icon.png and icon.rgba", SIZES)


if __name__ == "__main__":
    main()
