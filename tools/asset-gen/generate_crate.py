"""Procedurally generates a simple beveled crate mesh and exports it as a
single-mesh, single-primitive GLB, ready for `engine import` — a first,
concrete prototype of ADR-0009's "headless Blender scripting" pattern
(docs/decisions/0009-asset-generation-workflow.md).

No AI/ML model involved: this is a deterministic bpy script, the pattern
ADR-0009 recommends for reproducible/batch content, in the spirit of
Infinigen's "math rules only" approach.

Usage:
    blender --background --python tools/asset-gen/generate_crate.py -- --output <file.glb>
"""

import argparse
import sys

import bpy


def parse_args():
    argv = sys.argv
    if "--" in argv:
        argv = argv[argv.index("--") + 1 :]
    else:
        argv = []
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, help="Output .glb path")
    return parser.parse_args(argv)


def main():
    args = parse_args()

    # Start from a clean scene — the default cube/camera/light aren't needed
    # and would otherwise end up as extra meshes in the export.
    bpy.ops.wm.read_factory_settings(use_empty=True)

    bpy.ops.mesh.primitive_cube_add(size=1.0)
    crate = bpy.context.active_object
    crate.name = "Crate"

    bevel = crate.modifiers.new(name="Bevel", type="BEVEL")
    bevel.width = 0.06
    bevel.segments = 2
    bpy.ops.object.modifier_apply(modifier=bevel.name)

    material = bpy.data.materials.new(name="CrateMaterial")
    material.use_nodes = True
    bsdf = material.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (0.45, 0.30, 0.16, 1.0)
    crate.data.materials.append(material)

    bpy.ops.object.select_all(action="DESELECT")
    crate.select_set(True)
    bpy.context.view_layer.objects.active = crate

    bpy.ops.export_scene.gltf(
        filepath=args.output,
        export_format="GLB",
        use_selection=True,
        export_apply=True,
    )
    print(f"wrote {args.output}")


if __name__ == "__main__":
    main()
