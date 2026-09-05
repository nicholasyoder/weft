"""Covers generate_crate.py's parse_args() — pure argparse, zero real bpy
dependency. The module does `import bpy` at top level (Blender's embedded
module, not pip-installable), so bpy is stubbed before import; main()'s
actual Blender generation logic stays uncovered by design (ADR-0009's
workspace-boundary call — bpy only exists inside a real Blender install).
"""

import sys
from unittest import mock

sys.modules.setdefault("bpy", mock.MagicMock())

import generate_crate  # noqa: E402


def test_parse_args_defaults(monkeypatch):
    monkeypatch.setattr(sys, "argv", ["blender", "--", "--output", "out.glb"])
    args = generate_crate.parse_args()
    assert args.output == "out.glb"
    assert tuple(args.color) == (0.45, 0.30, 0.16)
    assert args.bevel_width == 0.06
    assert args.normal_map is False


def test_parse_args_normal_map_flag(monkeypatch):
    monkeypatch.setattr(
        sys, "argv", ["blender", "--", "--output", "out.glb", "--normal-map"]
    )
    args = generate_crate.parse_args()
    assert args.normal_map is True


def test_plank_normal_map_pixels_are_unit_length_and_flat_away_from_seams():
    width, height = 32, 32
    pixels = generate_crate.generate_plank_normal_map_pixels(width, height)
    assert len(pixels) == width * height * 4

    # A pixel at a ripple peak/trough should decode to a near-vertical
    # normal (0, 0, 1) — the cosine heightfield's slope is zero there.
    flat_u = 1 / (2 * generate_crate.PLANK_COUNT)  # a trough
    x = int(flat_u * width)
    idx = x * 4
    r, g, b, a = pixels[idx : idx + 4]
    assert abs(r - 0.5) < 1e-6
    assert abs(g - 0.5) < 1e-6
    assert b > 0.99
    assert a == 1.0

    # Every decoded normal should round-trip to a unit vector.
    for i in range(0, len(pixels), 4):
        nx = pixels[i] * 2 - 1
        ny = pixels[i + 1] * 2 - 1
        nz = pixels[i + 2] * 2 - 1
        length = (nx * nx + ny * ny + nz * nz) ** 0.5
        assert abs(length - 1.0) < 1e-5


def test_parse_args_color_and_bevel_width(monkeypatch):
    monkeypatch.setattr(
        sys,
        "argv",
        [
            "blender",
            "--",
            "--output",
            "out.glb",
            "--color",
            "0.5",
            "0.5",
            "0.55",
            "--bevel-width",
            "0.08",
        ],
    )
    args = generate_crate.parse_args()
    assert tuple(args.color) == (0.5, 0.5, 0.55)
    assert args.bevel_width == 0.08
