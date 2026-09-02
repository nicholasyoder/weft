use engine_assets::animation::{AnimationClip, JointTrack, Keyframes};
use engine_assets::skeleton::{Joint, Skeleton};
use glam::{Mat4, Quat, Vec3};

/// Samples `clip` against `skeleton` at `time` (seconds), returning one
/// skinning matrix per joint, in `skeleton`'s root-first order — each
/// entry is `joint_world * inverse_bind_matrix`, ready to upload as a GPU
/// joint palette.
///
/// A **pure function**: identical `(skeleton, clip, time)` always produces
/// identical output, with no dependency on tick count, RNG, or wall-clock
/// time. This is deliberate — it's what makes `animation_step` (which
/// calls this once per animated entity per tick) deterministic for free,
/// and what makes this function directly unit-testable without a `Sim`.
pub fn sample(skeleton: &Skeleton, clip: &AnimationClip, time: f32) -> Vec<Mat4> {
    let mut world = Vec::with_capacity(skeleton.joints.len());
    for (i, joint) in skeleton.joints.iter().enumerate() {
        let track = clip.tracks.iter().find(|t| t.joint as usize == i);
        let local = local_matrix(joint, track, time);
        let world_matrix = match joint.parent {
            Some(parent) => world[parent as usize] * local,
            None => local,
        };
        world.push(world_matrix);
    }
    world
        .into_iter()
        .zip(&skeleton.joints)
        .map(|(world_matrix, joint)| {
            world_matrix * Mat4::from_cols_array_2d(&joint.inverse_bind_matrix)
        })
        .collect()
}

fn local_matrix(joint: &Joint, track: Option<&JointTrack>, time: f32) -> Mat4 {
    let rest = &joint.local_rest_transform;
    let translation = track
        .and_then(|t| t.translation.as_ref())
        .map(|keys| sample_vec3(keys, time))
        .unwrap_or_else(|| Vec3::from(rest.translation));
    let rotation = track
        .and_then(|t| t.rotation.as_ref())
        .map(|keys| sample_quat(keys, time))
        .unwrap_or_else(|| Quat::from_array(rest.rotation));
    let scale = track
        .and_then(|t| t.scale.as_ref())
        .map(|keys| sample_vec3(keys, time))
        .unwrap_or_else(|| Vec3::from(rest.scale));
    Mat4::from_scale_rotation_translation(scale, rotation, translation)
}

/// The keyframe pair surrounding `time`: `(index_a, index_b, alpha)`.
/// `index_b` is `None` when `time` is at or beyond either boundary — in
/// that case the boundary keyframe's value should simply be held, not
/// extrapolated, matching how the rest of the engine treats out-of-range
/// input (clamp, not extrapolate).
fn segment(times: &[f32], time: f32) -> (usize, Option<usize>, f32) {
    let last = times.len() - 1;
    if time <= times[0] {
        return (0, None, 0.0);
    }
    if time >= times[last] {
        return (last, None, 0.0);
    }
    for i in 0..last {
        if time >= times[i] && time < times[i + 1] {
            let span = times[i + 1] - times[i];
            let alpha = if span > 0.0 {
                (time - times[i]) / span
            } else {
                0.0
            };
            return (i, Some(i + 1), alpha);
        }
    }
    (last, None, 0.0)
}

fn sample_vec3(keys: &Keyframes<[f32; 3]>, time: f32) -> Vec3 {
    let (a, b, t) = segment(&keys.times, time);
    let va = Vec3::from(keys.values[a]);
    match b {
        Some(bi) => va.lerp(Vec3::from(keys.values[bi]), t),
        None => va,
    }
}

fn sample_quat(keys: &Keyframes<[f32; 4]>, time: f32) -> Quat {
    let (a, b, t) = segment(&keys.times, time);
    let qa = Quat::from_array(keys.values[a]).normalize();
    match b {
        Some(bi) => qa.lerp(Quat::from_array(keys.values[bi]).normalize(), t),
        None => qa,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_trs() -> engine_assets::skeleton::Trs {
        engine_assets::skeleton::Trs {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }

    fn identity_mat4() -> [[f32; 4]; 4] {
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }

    /// A single root joint whose X translation is keyframed 0 -> 10 over
    /// t in [0, 1].
    fn moving_root_skeleton_and_clip() -> (Skeleton, AnimationClip) {
        let skeleton = Skeleton {
            joints: vec![Joint {
                parent: None,
                inverse_bind_matrix: identity_mat4(),
                local_rest_transform: identity_trs(),
            }],
        };
        let clip = AnimationClip {
            duration: 1.0,
            tracks: vec![JointTrack {
                joint: 0,
                translation: Some(Keyframes {
                    times: vec![0.0, 1.0],
                    values: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]],
                }),
                rotation: None,
                scale: None,
            }],
        };
        (skeleton, clip)
    }

    #[test]
    fn same_input_twice_produces_identical_output() {
        let (skeleton, clip) = moving_root_skeleton_and_clip();
        let a = sample(&skeleton, &clip, 0.37);
        let b = sample(&skeleton, &clip, 0.37);
        assert_eq!(a, b);
    }

    #[test]
    fn interpolates_at_the_midpoint_between_two_keyframes() {
        let (skeleton, clip) = moving_root_skeleton_and_clip();
        let matrices = sample(&skeleton, &clip, 0.5);
        let translation = matrices[0].w_axis.truncate();
        assert!((translation.x - 5.0).abs() < 1e-5, "got {translation:?}");
    }

    #[test]
    fn holds_the_boundary_value_before_first_and_after_last_keyframe() {
        let (skeleton, clip) = moving_root_skeleton_and_clip();
        let before = sample(&skeleton, &clip, -1.0);
        let after = sample(&skeleton, &clip, 5.0);
        assert!((before[0].w_axis.x - 0.0).abs() < 1e-5);
        assert!((after[0].w_axis.x - 10.0).abs() < 1e-5);
    }

    #[test]
    fn a_joint_with_no_track_falls_back_to_its_rest_pose() {
        let skeleton = Skeleton {
            joints: vec![Joint {
                parent: None,
                inverse_bind_matrix: identity_mat4(),
                local_rest_transform: engine_assets::skeleton::Trs {
                    translation: [1.0, 2.0, 3.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                },
            }],
        };
        let clip = AnimationClip {
            duration: 1.0,
            tracks: vec![],
        };
        let matrices = sample(&skeleton, &clip, 0.5);
        assert_eq!(matrices[0].w_axis.truncate(), Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn a_child_joint_composes_with_its_parent_world_transform() {
        let skeleton = Skeleton {
            joints: vec![
                Joint {
                    parent: None,
                    inverse_bind_matrix: identity_mat4(),
                    local_rest_transform: engine_assets::skeleton::Trs {
                        translation: [10.0, 0.0, 0.0],
                        ..identity_trs()
                    },
                },
                Joint {
                    parent: Some(0),
                    inverse_bind_matrix: identity_mat4(),
                    local_rest_transform: engine_assets::skeleton::Trs {
                        translation: [0.0, 1.0, 0.0],
                        ..identity_trs()
                    },
                },
            ],
        };
        let clip = AnimationClip {
            duration: 1.0,
            tracks: vec![],
        };
        let matrices = sample(&skeleton, &clip, 0.0);
        assert_eq!(matrices[1].w_axis.truncate(), Vec3::new(10.0, 1.0, 0.0));
    }
}
