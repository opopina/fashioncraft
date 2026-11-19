//! BVH import: a small skeleton written here (T pose at rest, the left arm lowered over one
//! second, the hips turned) is read and its motion retargeted onto the mannequin's rig, whose
//! arms rest lowered: the avatar's bones point where the file's do.
use fashioncraft::{
    avatar::SurfaceMesh,
    avatar_rig::Rig,
    bvh_import::{parse, retarget},
    garment::arrangement::load_sidecar,
};
use glam::DVec3;
use std::path::PathBuf;

/// A T-pose skeleton facing +Z, centimetres, with `frames` lines of channel values.
fn file(frames: &[&str], frame_time: f64) -> String {
    let mut text = String::from(
        "HIERARCHY
ROOT Hips
{
  OFFSET 0 95 0
  CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation
  JOINT Spine
  {
    OFFSET 0 10 0
    CHANNELS 3 Zrotation Xrotation Yrotation
    JOINT Spine2
    {
      OFFSET 0 20 0
      CHANNELS 3 Zrotation Xrotation Yrotation
      JOINT Neck
      {
        OFFSET 0 20 0
        CHANNELS 3 Zrotation Xrotation Yrotation
        JOINT Head
        {
          OFFSET 0 10 0
          CHANNELS 3 Zrotation Xrotation Yrotation
          End Site
          {
            OFFSET 0 15 0
          }
        }
      }
      JOINT LeftShoulder
      {
        OFFSET 5 15 0
        CHANNELS 3 Zrotation Xrotation Yrotation
        JOINT LeftArm
        {
          OFFSET 12 0 0
          CHANNELS 3 Zrotation Xrotation Yrotation
          JOINT LeftForeArm
          {
            OFFSET 28 0 0
            CHANNELS 3 Zrotation Xrotation Yrotation
            JOINT LeftHand
            {
              OFFSET 25 0 0
              CHANNELS 3 Zrotation Xrotation Yrotation
              End Site
              {
                OFFSET 15 0 0
              }
            }
          }
        }
      }
      JOINT RightShoulder
      {
        OFFSET -5 15 0
        CHANNELS 3 Zrotation Xrotation Yrotation
        JOINT RightArm
        {
          OFFSET -12 0 0
          CHANNELS 3 Zrotation Xrotation Yrotation
          JOINT RightForeArm
          {
            OFFSET -28 0 0
            CHANNELS 3 Zrotation Xrotation Yrotation
            JOINT RightHand
            {
              OFFSET -25 0 0
              CHANNELS 3 Zrotation Xrotation Yrotation
              End Site
              {
                OFFSET -15 0 0
              }
            }
          }
        }
      }
    }
  }
  JOINT LeftUpLeg
  {
    OFFSET 10 0 0
    CHANNELS 3 Zrotation Xrotation Yrotation
    JOINT LeftLeg
    {
      OFFSET 0 -45 0
      CHANNELS 3 Zrotation Xrotation Yrotation
      JOINT LeftFoot
      {
        OFFSET 0 -42 0
        CHANNELS 3 Zrotation Xrotation Yrotation
        End Site
        {
          OFFSET 0 -5 15
        }
      }
    }
  }
  JOINT RightUpLeg
  {
    OFFSET -10 0 0
    CHANNELS 3 Zrotation Xrotation Yrotation
    JOINT RightLeg
    {
      OFFSET 0 -45 0
      CHANNELS 3 Zrotation Xrotation Yrotation
      JOINT RightFoot
      {
        OFFSET 0 -42 0
        CHANNELS 3 Zrotation Xrotation Yrotation
        End Site
        {
          OFFSET 0 -5 15
        }
      }
    }
  }
}
MOTION
",
    );
    text += &format!("Frames: {}\nFrame Time: {frame_time}\n", frames.len());
    for f in frames {
        text += f;
        text += "\n";
    }
    text
}

/// One frame's 60 channel values: hips position and turn, then every joint's turn, with
/// the left arm (LeftArm) turned `arm` degrees about Z and the hips `hips` degrees about Y.
fn frame(arm: f64, hips: f64) -> String {
    let mut v = vec![0.; 60];
    v[1] = 95.;
    v[5] = hips;
    // Joint order: Hips(6) Spine Spine2 Neck Head LeftShoulder LeftArm…
    let left_arm = 6 + 3 * 5;
    v[left_arm] = arm;
    v.iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn rig() -> (Rig, SurfaceMesh) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let joints = load_sidecar(&path)
        .unwrap()
        .unwrap()
        .landmarks
        .into_iter()
        .map(|l| (l.id, l.position_m))
        .collect();
    let surface = SurfaceMesh::load_obj(&path).unwrap();
    (Rig::new(&joints, &surface).unwrap(), surface)
}

#[test]
fn a_bvh_motion_lands_on_the_avatar() {
    let frames: Vec<String> = (0..=30)
        .map(|k| frame(-80. * k as f64 / 30., 30. * k as f64 / 30.))
        .collect();
    let refs: Vec<&str> = frames.iter().map(String::as_str).collect();
    let bvh = parse(&file(&refs, 1. / 30.)).unwrap();
    assert_eq!(bvh.frame_count(), 31);
    let (rig, _) = rig();
    let animation = retarget(&bvh, &rig).unwrap();
    assert_eq!(animation.keys.len(), 31);
    assert!((animation.keys.last().unwrap().time_s - 1.).abs() < 1e-9);
    let bone = |pose: &fashioncraft::avatar_rig::Pose, name: &str| {
        let k = rig.bone(name).unwrap();
        let (head, tail) = rig.posed_bones(pose)[k];
        (tail - head).normalize()
    };
    // At the start the left arm reaches out sideways as in the file's T pose…
    let start = &animation.keys[0].pose;
    assert!(bone(start, "upper_arm.L").dot(DVec3::X) > 0.99);
    assert!(bone(start, "upper_arm.R").dot(-DVec3::X) > 0.99);
    // …and at the end it hangs 80° lower, turned with the hips 30° about the vertical.
    let end = &animation.keys[30].pose;
    let turn = glam::DQuat::from_rotation_y(30f64.to_radians());
    let lowered = turn * (glam::DQuat::from_rotation_z((-80f64).to_radians()) * DVec3::X);
    assert!(
        bone(end, "upper_arm.L").dot(lowered) > 0.99,
        "{:?}",
        bone(end, "upper_arm.L")
    );
    // The legs follow the hips' turn and still point down.
    assert!(bone(end, "thigh.L").dot(-DVec3::Y) > 0.99);
}

#[test]
fn files_that_are_not_bvh_are_refused() {
    assert!(parse("not a bvh").is_err());
    assert!(
        parse("HIERARCHY\nROOT Hips\n{\nOFFSET 0 0 0\nCHANNELS 1 Wobble\n}\nMOTION\n").is_err()
    );
    let one = frame(0., 0.);
    let truncated = file(&[&one[..20]], 1. / 30.);
    assert!(parse(&truncated).is_err());
}
