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
