#!/usr/bin/env python3
"""Measure one opaque sRGB hex color pair; not a page accessibility audit."""
import argparse
import json
import math
import re


def parse_hex(value):
    if not isinstance(value, str) or not re.fullmatch(r"#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})", value):
        raise ValueError("Use opaque sRGB #RGB or #RRGGBB; resolve alpha/backgrounds before checking.")
    value = value[1:]
    if len(value) == 3:
        value = "".join(c * 2 for c in value)
    return tuple(int(value[i:i + 2], 16) / 255 for i in (0, 2, 4))


def luminance(rgb):
    linear = [c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in rgb]
    return sum(c * w for c, w in zip(linear, (0.2126, 0.7152, 0.0722)))


def contrast(first, second):
    a, b = sorted((luminance(parse_hex(first)), luminance(parse_hex(second))))
    return (b + 0.05) / (a + 0.05)


def self_test():
    assert math.isclose(contrast("#000", "#fff"), 21.0)
    assert math.isclose(contrast("#fff", "#ffffff"), 1.0)
    assert math.isclose(contrast("#123", "#fff"), contrast("#FFF", "#112233"))
    assert 4.47 < contrast("#777", "#fff") < 4.5
    assert contrast("#767676", "#fff") > 4.5
    assert math.isclose(luminance(parse_hex("#ff0000")), 0.2126)
    for invalid in ("red", "#1234", "#11223344", "#ggg", "fff", "#1234567"):
        try:
            parse_hex(invalid)
        except ValueError:
            continue
        raise AssertionError("Unsupported input accepted: " + invalid)
    print("PASS: reference pairs, symmetry, shorthand, threshold boundary, luminance, and invalid input")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("foreground", nargs="?")
    parser.add_argument("background", nargs="?")
    parser.add_argument("--minimum", type=float, help="Optional required ratio 1..21; fail exit code 1 if below it")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.foreground is None or args.background is None:
        parser.error("foreground and background are required unless using --self-test")
    if args.minimum is not None and (not math.isfinite(args.minimum) or not 1 <= args.minimum <= 21):
        parser.error("--minimum must be a finite number from 1 to 21")
    try:
        ratio = contrast(args.foreground, args.background)
    except ValueError as exc:
        parser.error(str(exc))
    result = {
        "foreground": args.foreground,
        "background": args.background,
        "contrast_ratio": ratio,
        "normal_text_AA_pair": ratio >= 4.5,
        "large_text_AA_pair": ratio >= 3.0,
        "non_text_pair_where_applicable": ratio >= 3.0,
        "scope": "Opaque sRGB pair only; ignores layout, alpha, images, gradients, font rendering, and interaction."
    }
    if args.minimum is not None:
        result["minimum"] = args.minimum
        result["passes_minimum"] = ratio >= args.minimum
    print(json.dumps(result, indent=2))
    return 1 if args.minimum is not None and ratio < args.minimum else 0


if __name__ == "__main__":
    raise SystemExit(main())
