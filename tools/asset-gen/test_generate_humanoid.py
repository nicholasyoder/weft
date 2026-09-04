"""Covers generate_humanoid.py's parse_args() — pure argparse, zero real bpy
dependency. The module does `import bpy` at top level (Blender's embedded
module, not pip-installable), so bpy is stubbed before import; main()'s
actual Blender generation logic stays uncovered by design (ADR-0009's
workspace-boundary call — bpy only exists inside a real Blender install).
"""

import sys
from unittest import mock

sys.modules.setdefault("bpy", mock.MagicMock())

import generate_humanoid  # noqa: E402


def test_parse_args_defaults(monkeypatch):
    monkeypatch.setattr(sys, "argv", ["blender", "--", "--output", "out.glb"])
    args = generate_humanoid.parse_args()
    assert args.output == "out.glb"
    assert tuple(args.color) == (0.85, 0.2, 0.2)


def test_parse_args_color(monkeypatch):
    monkeypatch.setattr(
        sys,
        "argv",
        [
            "blender",
            "--",
            "--output",
            "out.glb",
            "--color",
            "0.1",
            "0.2",
            "0.3",
        ],
    )
    args = generate_humanoid.parse_args()
    assert tuple(args.color) == (0.1, 0.2, 0.3)
