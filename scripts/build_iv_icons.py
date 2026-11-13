#!/usr/bin/env python3
"""Build editable SVG, PNG, Windows ICO and macOS ICNS application icons.

Requires Pillow only. The same geometry generates the vector and raster assets.
"""

from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / "assets"
BLUE = "#55ADFF"
ORANGE = "#FFAD58"
CURVES = [
    (
        BLUE,
        [
            (130, 372),
            (213, 364),
            (222, 295),
            (254, 258),
            (286, 222),
            (301, 152),
            (384, 136),
        ],
    ),
    (
        ORANGE,
        [
            (130, 394),
            (223, 391),
            (236, 318),
            (272, 281),
            (310, 244),
            (336, 175),
            (390, 151),
        ],
    ),
]


def bezier(points):
    result = []
    for segment in (points[:4], points[3:]):
        for index in range(161):
            t = index / 160
            weights = [(1 - t) ** 3, 3 * (1 - t) ** 2 * t, 3 * (1 - t) * t * t, t**3]
            result.append(
                tuple(
                    sum(w * p[axis] for w, p in zip(weights, segment))
                    for axis in (0, 1)
                )
            )
    return result


def main():
    DEST.mkdir(parents=True, exist_ok=True)
    paths = []
    for color, points in CURVES:
        coords = [f"{x} {y}" for x, y in points]
        paths.append(
            f'<path d="M {coords[0]} C {" ".join(coords[1:4])} C {" ".join(coords[4:])}" stroke="{color}"/>'
        )
    svg = (
        """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="1024" height="1024">
  <title>KohdaLab IV application icon</title>
  <defs><linearGradient id="bg" x2="0" y2="1"><stop stop-color="#172333"/><stop offset="1" stop-color="#080F19"/></linearGradient></defs>
  <rect x="20" y="20" width="472" height="472" rx="108" fill="url(#bg)" stroke="#2B3B50" stroke-width="3"/>
  <path d="M 116 118 V 409 H 407" fill="none" stroke="#B7CADF" stroke-opacity="0.35" stroke-width="9" stroke-linecap="round" stroke-linejoin="round"/>
  <g fill="none" stroke-width="27" stroke-linecap="round" stroke-linejoin="round">
    """
        + "\n    ".join(paths)
        + """
  </g>
</svg>
"""
    )
    (DEST / "kohdalab-iv.svg").write_text(svg, encoding="utf-8")
    scale = 4
    edge = 512 * scale
    image = Image.new("RGBA", (edge, edge))
    mask = Image.new("L", (edge, edge))
    box = tuple(int(v * scale) for v in (20, 20, 492, 492))
    ImageDraw.Draw(mask).rounded_rectangle(box, radius=108 * scale, fill=255)
    top, bottom = (23, 35, 51), (8, 15, 25)
    background = Image.new("RGBA", (edge, edge))
    draw = ImageDraw.Draw(background)
    for y in range(edge):
        fraction = min(1, max(0, (y / scale - 20) / 472))
        color = tuple(round(a + (b - a) * fraction) for a, b in zip(top, bottom)) + (
            255,
        )
        draw.line((0, y, edge, y), fill=color)
    image.paste(background, (0, 0), mask)
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle(box, radius=108 * scale, outline="#2B3B50", width=3 * scale)

    def stroke(points, color, width):
        scaled = [(round(x * scale), round(y * scale)) for x, y in points]
        draw.line(scaled, fill=color, width=round(width * scale), joint="curve")
        radius = width * scale / 2
        for x, y in (scaled[0], scaled[-1]):
            draw.ellipse(
                (
                    round(x - radius),
                    round(y - radius),
                    round(x + radius),
                    round(y + radius),
                ),
                fill=color,
            )

    # Composite the faint axes so raster and SVG share the same opacity.
    axes = Image.new("RGBA", image.size)
    axes_draw = ImageDraw.Draw(axes)
    axes_draw.line(
        [
            (116 * scale, 118 * scale),
            (116 * scale, 409 * scale),
            (407 * scale, 409 * scale),
        ],
        fill=(183, 202, 223, 89),
        width=9 * scale,
        joint="curve",
    )
    image = Image.alpha_composite(image, axes)
    draw = ImageDraw.Draw(image)
    for color, points in CURVES:
        stroke(bezier(points), color, 27)
    master = image.resize((1024, 1024), Image.Resampling.LANCZOS)
    master.save(DEST / "kohdalab-iv.png")
    master.resize((256, 256), Image.Resampling.LANCZOS).save(
        DEST / "kohdalab-iv-window.png"
    )
    master.save(
        DEST / "kohdalab-iv.ico", sizes=[(n, n) for n in (16, 24, 32, 48, 64, 128, 256)]
    )
    master.save(DEST / "kohdalab-iv.icns")
    print(f"Application icons written to {DEST}")


if __name__ == "__main__":
    main()
