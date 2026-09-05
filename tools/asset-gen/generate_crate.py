"""Procedurally generates a simple beveled crate mesh and exports it as a
single-mesh, single-primitive GLB, ready for `engine import` — a first,
concrete prototype of ADR-0009's "headless Blender scripting" pattern
(docs/decisions/0009-asset-generation-workflow.md).

No AI/ML model involved: this is a deterministic bpy script, the pattern
ADR-0009 recommends for reproducible/batch content, in the spirit of
Infinigen's "math rules only" approach. --color/--bevel-width are exposed
so the same script can deterministically produce a small kit of crate
variants, not just one fixed asset.

--normal-map (games/sandbox's "expand the sandbox" pass, ADR-0019 Phase 3)
optionally adds a procedural wood-panel normal map: a pure-math heightfield
(a cosine ripple, no noise/randomness — deterministic like everything else
this script generates), converted to a tangent-space normal map via a
central-difference gradient, computed and packed entirely with bpy's own
image pixel buffer — no PIL/numpy dependency, no external API, consistent
with this project's "no hardcoded external APIs" asset-generation
constraint. glTF/this engine's importer only understand a *baked image*
normal map, not a procedural Blender node graph, so this has to produce a
real texture, not just a node hookup. A continuous wave (rather than sharp
grooves at discrete seams) was chosen deliberately after the sharp-seam
version turned out to only affect a couple of near-saturated pixels per
seam — invisible at typical in-game viewing distance; the whole-face ripple
is visible from across the arena.

Usage:
    blender --background --python tools/asset-gen/generate_crate.py -- --output <file.glb> [--color R G B] [--bevel-width W] [--normal-map]
"""

import argparse
import math
import sys

import bpy

NORMAL_MAP_SIZE = 256
# How many ripples wrap around the crate's U axis.
PLANK_COUNT = 4
# Heightfield amplitude, in the same arbitrary units the gradient is
# computed from (not world units — only the resulting normal directions,
# not this scale, ever reach the mesh). Tuned so the normal map's peak
# tilt away from (0, 0, 1) is moderate (roughly +-0.6 in the X channel)
# rather than fully saturated.
WAVE_AMPLITUDE = 0.03


def parse_args():
    argv = sys.argv
    if "--" in argv:
        argv = argv[argv.index("--") + 1 :]
    else:
        argv = []
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, help="Output .glb path")
    parser.add_argument(
        "--color",
        type=float,
        nargs=3,
        default=(0.45, 0.30, 0.16),
        metavar=("R", "G", "B"),
        help="Base color, each channel 0.0-1.0 (default: the original crate brown)",
    )
    parser.add_argument(
        "--bevel-width",
        type=float,
        default=0.06,
        help="Edge bevel width (default: 0.06)",
    )
    parser.add_argument(
        "--normal-map",
        action="store_true",
        help="Also generate and assign a procedural plank-groove normal map",
    )
    return parser.parse_args(argv)


def _plank_groove_heights(width, height):
    """A 1D-along-U cosine-ripple heightfield, repeated down every row —
    PLANK_COUNT full ripples wrapping around U. Returns a flat list of
    `width * height` floats.
    """
    row = [
        WAVE_AMPLITUDE * math.cos(2.0 * math.pi * PLANK_COUNT * (x / width))
        for x in range(width)
    ]
    return row * height


def generate_plank_normal_map_pixels(width, height):
    """RGBA float pixels (bpy `Image.pixels` order: row 0 first, straight
    alpha) encoding a tangent-space normal map for `_plank_groove_heights`,
    via a central-difference gradient along U (the ripple runs along V, so
    there's no V-gradient to compute).
    """
    heights = _plank_groove_heights(width, height)
    pixels = [0.0] * (width * height * 4)
    for y in range(height):
        row_offset = y * width
        for x in range(width):
            x0 = (x - 1) % width
            x1 = (x + 1) % width
            h0 = heights[row_offset + x0]
            h1 = heights[row_offset + x1]
            dhdu = (h1 - h0) * width * 0.5
            nx, ny, nz = -dhdu, 0.0, 1.0
            length = math.sqrt(nx * nx + ny * ny + nz * nz)
            idx = (row_offset + x) * 4
            pixels[idx] = nx / length * 0.5 + 0.5
            pixels[idx + 1] = ny / length * 0.5 + 0.5
            pixels[idx + 2] = nz / length * 0.5 + 0.5
            pixels[idx + 3] = 1.0
    return pixels


def add_normal_map(material, bsdf):
    """Builds the procedural normal-map image, packs it into the .blend so
    the glTF exporter can embed it in the GLB with no on-disk file
    dependency, and wires Image Texture -> Normal Map -> BSDF.Normal.
    `colorspace_settings.name = "Non-Color"` is required — Blender defaults
    new images to sRGB, which would gamma-correct the normal-encoded
    channels and corrupt every normal direction. **Must be set before
    `.pixels` is assigned, not after**: setting it afterward silently resets
    the image back to a blank/generated buffer, discarding every pixel just
    written (verified empirically against this project's pinned Blender
    version — costly to rediscover, so recorded here).
    """
    image = bpy.data.images.new(
        "CrateNormalMap", width=NORMAL_MAP_SIZE, height=NORMAL_MAP_SIZE
    )
    image.colorspace_settings.name = "Non-Color"
    image.pixels[:] = generate_plank_normal_map_pixels(NORMAL_MAP_SIZE, NORMAL_MAP_SIZE)
    image.pack()

    tree = material.node_tree
    tex_node = tree.nodes.new("ShaderNodeTexImage")
    tex_node.image = image
    normal_map_node = tree.nodes.new("ShaderNodeNormalMap")
    tree.links.new(tex_node.outputs["Color"], normal_map_node.inputs["Color"])
    tree.links.new(normal_map_node.outputs["Normal"], bsdf.inputs["Normal"])


def main():
    args = parse_args()

    # Start from a clean scene — the default cube/camera/light aren't needed
    # and would otherwise end up as extra meshes in the export.
    bpy.ops.wm.read_factory_settings(use_empty=True)

    bpy.ops.mesh.primitive_cube_add(size=1.0)
    crate = bpy.context.active_object
    crate.name = "Crate"

    bevel = crate.modifiers.new(name="Bevel", type="BEVEL")
    bevel.width = args.bevel_width
    bevel.segments = 2
    bpy.ops.object.modifier_apply(modifier=bevel.name)

    material = bpy.data.materials.new(name="CrateMaterial")
    material.use_nodes = True
    bsdf = material.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*args.color, 1.0)
    if args.normal_map:
        add_normal_map(material, bsdf)
    crate.data.materials.append(material)

    bpy.ops.object.select_all(action="DESELECT")
    crate.select_set(True)
    bpy.context.view_layer.objects.active = crate

    bpy.ops.export_scene.gltf(
        filepath=args.output,
        export_format="GLB",
        use_selection=True,
        export_apply=True,
        export_tangents=True,
    )
    print(f"wrote {args.output}")


if __name__ == "__main__":
    main()
