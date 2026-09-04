"""Procedurally generates a simple low-poly humanoid mesh (legs, torso,
head, arms, a small nose marker for facing) and exports it as a
single-mesh, single-primitive GLB, ready for `engine import` — same
headless-Blender pattern as `generate_crate.py` (ADR-0009).

No AI/ML model involved: deterministic bpy primitives only. All parts are
built at the same scale as `games/sandbox`'s player capsule collider
(half_height=0.5, radius=0.3 — total vertical extent 1.6, from z=-0.8 to
z=+0.8 in Blender's Z-up authoring space) so the mesh can be dropped
straight into the scene at Transform.scale = [1, 1, 1], no scale hack
needed.

**Facing convention, verified empirically against this project's pinned
Blender version, not assumed**: Blender's default glTF export maps
Blender's local +Y axis to the engine's -Z axis (glTF's forward/Y-up
convention: glTF_x = blender_x, glTF_y = blender_z, glTF_z = -blender_y).
Since `games/sandbox`'s W key moves in engine `Vec3::NEG_Z`, this humanoid
is built facing Blender +Y so its identity-rotation rest pose visually
faces the direction W moves it. The small "nose" box (built forward of the
head, along +Y) exists purely so that orientation is visually checkable in
a render, not just asserted.

Usage:
    blender --background --python tools/asset-gen/generate_humanoid.py -- --output <file.glb> [--color R G B]
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
    parser.add_argument(
        "--color",
        type=float,
        nargs=3,
        default=(0.85, 0.2, 0.2),
        metavar=("R", "G", "B"),
        help="Base color, each channel 0.0-1.0 (default: matches the sandbox player's original red)",
    )
    return parser.parse_args(argv)


def add_box(name, size, location):
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=location)
    obj = bpy.context.active_object
    obj.name = name
    obj.scale = size
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return obj


def add_sphere(name, radius, location):
    bpy.ops.mesh.primitive_uv_sphere_add(radius=radius, location=location)
    obj = bpy.context.active_object
    obj.name = name
    return obj


def main():
    args = parse_args()

    # Start from a clean scene — the default cube/camera/light aren't needed
    # and would otherwise end up as extra meshes in the export.
    bpy.ops.wm.read_factory_settings(use_empty=True)

    parts = [
        # Legs: two boxes, feet at z=-0.8 (matching the capsule's bottom
        # cap), hips at z=-0.18.
        add_box("LegLeft", (0.14, 0.14, 0.62), (-0.09, 0.0, -0.49)),
        add_box("LegRight", (0.14, 0.14, 0.62), (0.09, 0.0, -0.49)),
        # Torso: hips at z=-0.18, shoulders at z=0.24.
        add_box("Torso", (0.46, 0.24, 0.42), (0.0, 0.0, 0.03)),
        # Arms: hanging at the torso's sides, shoulders to roughly hip
        # height.
        add_box("ArmLeft", (0.13, 0.13, 0.44), (-0.30, 0.0, 0.02)),
        add_box("ArmRight", (0.13, 0.13, 0.44), (0.30, 0.0, 0.02)),
        # Head: centered well clear of the torso, top at z=0.66, inside the
        # capsule's z=0.8 top cap.
        add_sphere("Head", 0.17, (0.0, 0.0, 0.49)),
        # A small forward-facing nose marker — the only asymmetric part of
        # an otherwise front/back-symmetric mesh, so front-vs-back facing
        # is visually checkable in a render, not just asserted.
        add_box("Nose", (0.06, 0.05, 0.05), (0.0, 0.17, 0.52)),
    ]

    bpy.ops.object.select_all(action="DESELECT")
    for part in parts:
        part.select_set(True)
    bpy.context.view_layer.objects.active = parts[2]  # Torso
    bpy.ops.object.join()
    humanoid = bpy.context.active_object
    humanoid.name = "Humanoid"

    material = bpy.data.materials.new(name="HumanoidMaterial")
    material.use_nodes = True
    bsdf = material.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*args.color, 1.0)
    humanoid.data.materials.append(material)

    bpy.ops.object.select_all(action="DESELECT")
    humanoid.select_set(True)
    bpy.context.view_layer.objects.active = humanoid

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
