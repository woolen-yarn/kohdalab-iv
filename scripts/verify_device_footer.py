"""Capture the fixed device footer and the shared device cards in both layouts.

Footer cases: many collapsed cards, with and without scrolling; the Add
device footer must not move and every collapsed card must have one size.
Card cases: the default Source and Meter plus one added Source and Meter,
all with Device and Settings open; every card must have one height and the same equal
Connect | Disconnect | Settings row and Settings rows below the live plot.
Simulation only: no instrument is opened.
"""
import argparse
import json
from pathlib import Path
import subprocess


def capture(binary, screenshot, *flags):
    command = [str(binary), "--gui-footer-smoke", *flags, "--screenshot", str(screenshot)]
    result = subprocess.run(command, capture_output=True, text=True, timeout=30)
    if result.returncode:
        raise RuntimeError(result.stderr)
    report = next(json.loads(line) for line in result.stdout.splitlines()
                  if '"footer_geometry"' in line)
    if report["points"] != 0 or not screenshot.is_file():
        raise RuntimeError(f"Invalid capture {screenshot.name}: {report}")
    return report


def width(rect):
    return rect[2] - rect[0]


def height(rect):
    return rect[3] - rect[1]


def check_columns(report, columns, cards):
    if report["two_columns"] != (columns == 2):
        raise RuntimeError(f"Layout is not {columns} column(s): {report['two_columns']}")
    for kind in (True, False):
        widths = [width(card["card"]) for card in cards if card["source"] == kind]
        if max(widths) - min(widths) > 0.5:
            raise RuntimeError(f"Cards of one kind differ in width: {widths}")
    if columns == 2:
        right_of_sources = max(card["card"][2] for card in cards if card["source"])
        left_of_meters = min(card["card"][0] for card in cards if not card["source"])
        if right_of_sources > left_of_meters:
            raise RuntimeError("Two-column layout must place Sources left and Meters right")


def check_footer(binary, output):
    for columns in (1, 2):
        positions = []
        for scrolled in (False, True):
            suffix = "after" if scrolled else "before"
            screenshot = output / f"iv-{columns}-column-{suffix}.png"
            flags = []
            if columns == 2:
                flags.append("--footer-two-columns")
            if scrolled:
                flags.append("--footer-scrolled")
            report = capture(binary, screenshot, *flags)
            offset, top, bottom = report["footer_geometry"]
            if (not 0 <= report["window_bottom"] - bottom < 32
                    or bottom - top < 20 or (offset > 100) != scrolled):
                raise RuntimeError(f"Invalid device footer capture: {report}")
            cards = report["cards"]
            heights = [height(card["card"]) for card in cards]
            if len(cards) != 14 or max(heights) - min(heights) > 0.5:
                raise RuntimeError(f"Collapsed cards must share one height: {heights}")
            if any(card["connect"] for card in cards):
                raise RuntimeError("Collapsed cards must not show the connection row")
            check_columns(report, columns, cards)
            positions.append((top, bottom))
            print(f"{screenshot.name}: scroll={offset:.1f}, footer={top:.1f}–{bottom:.1f}, "
                  f"card height={heights[0]:.1f}")
        if any(abs(before - after) > 1 for before, after in zip(*positions)):
            raise RuntimeError("The Add device footer moved with the cards")


def check_cards(binary, output):
    for columns in (1, 2):
        screenshot = output / f"iv-{columns}-column-cards.png"
        flags = ["--footer-cards"] + (["--footer-two-columns"] if columns == 2 else [])
        report = capture(binary, screenshot, *flags)
        cards = report["cards"]
        roles = sorted((card["source"], card["role"]) for card in cards)
        expected = sorted([(True, "Source1"), (True, "Unassigned"),
                           (False, "Meter"), (False, "Unassigned")])
        if roles != expected:
            raise RuntimeError(f"Expected default and added Source/Meter cards: {roles}")
        for card in cards:
            frame = card["card"]
            buttons = [card[name] for name in ("connect", "disconnect", "settings")]
            rows, plot = card["settings_rows"], card["plot"]
            if not all(buttons) or not rows or not plot:
                raise RuntimeError(f"Card {card['id']} lacks Device or Settings controls: {card}")
            widths = [width(button) for button in buttons]
            if max(widths) - min(widths) > 0.5 or len({round(b[1], 1) for b in buttons}) != 1:
                raise RuntimeError(f"Card {card['id']} buttons are not one equal row: {buttons}")
            for rect in [*buttons, rows]:
                if rect[0] < frame[0] - 0.5 or rect[2] > frame[2] + 0.5 or rect[3] > frame[3] + 0.5:
                    raise RuntimeError(f"Card {card['id']} control leaves the card: {rect} {frame}")
            if not buttons[0][3] < plot[1] <= rows[1]:
                raise RuntimeError(f"Card {card['id']} order must be Device row, plot, Settings")
        heights = [height(card["card"]) for card in cards]
        if max(heights) - min(heights) > 0.5:
            raise RuntimeError(f"Open Source/Meter cards must share one height: {heights}")
        check_columns(report, columns, cards)
        print(f"{screenshot.name}: {len(cards)} cards, height {heights[0]:.1f}, buttons "
              f"{width(cards[0]['connect']):.1f} px, Settings open")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    check_footer(binary, output)
    check_cards(binary, output)


if __name__ == "__main__":
    main()
