use webrtc_constraints::{
    property::name::*, AdvancedMediaTrackConstraints, BareOrMediaTrackConstraintSet,
    BareOrValueRangeConstraint, BoolOrMediaTrackConstraints, MediaStreamConstraints,
    MediaTrackConstraints, ValueRangeConstraint,
};

// https://www.w3.org/TR/mediacapture-streams/#example-1
#[cfg(feature = "serde")]
#[test]
fn w3c_spec_example_1() {
    let actual: MediaStreamConstraints = {
        let json = serde_json::json!({
            "video": {
                "width": 1280,
                "height": 720,
                "aspectRatio": 1.5,
            }
        });
        serde_json::from_value(json).unwrap()
    };
    let expected = MediaStreamConstraints {
        audio: BoolOrMediaTrackConstraints::Bool(false),
        video: BoolOrMediaTrackConstraints::Constraints(MediaTrackConstraints {
            basic: BareOrMediaTrackConstraintSet::from_iter([
                (WIDTH, 1280.into()),
                (HEIGHT, 720.into()),
                (ASPECT_RATIO, 1.5.into()),
            ]),
            advanced: AdvancedMediaTrackConstraints::default(),
        }),
    };

    assert_eq!(actual, expected);
}

// https://www.w3.org/TR/mediacapture-streams/#example-2
#[cfg(feature = "serde")]
#[test]
fn w3c_spec_example_2() {
    let actual: MediaStreamConstraints = {
        let json = serde_json::json!({
            "video": {
                "width": { "min": 640, "ideal": 1280 },
                "height": { "min": 480, "ideal": 720 },
                "aspectRatio": 1.5,
                "frameRate": { "min": 20.0 },
            }
        });
        serde_json::from_value(json).unwrap()
    };

    let expected = MediaStreamConstraints {
        audio: BoolOrMediaTrackConstraints::Bool(false),
        video: BoolOrMediaTrackConstraints::Constraints(MediaTrackConstraints {
            basic: BareOrMediaTrackConstraintSet::from_iter([
                (
                    WIDTH,
                    BareOrValueRangeConstraint::Constraint(ValueRangeConstraint {
                        min: Some(640),
                        max: None,
                        exact: None,
                        ideal: Some(1280),
                    })
                    .into(),
                ),
                (
                    HEIGHT,
                    BareOrValueRangeConstraint::Constraint(ValueRangeConstraint {
                        min: Some(480),
                        max: None,
                        exact: None,
                        ideal: Some(720),
                    })
                    .into(),
                ),
                (ASPECT_RATIO, BareOrValueRangeConstraint::Bare(1.5).into()),
                (
                    FRAME_RATE,
                    BareOrValueRangeConstraint::Constraint(ValueRangeConstraint {
                        min: Some(20.0),
                        max: None,
                        exact: None,
                        ideal: None,
                    })
                    .into(),
                ),
            ]),
            advanced: AdvancedMediaTrackConstraints::default(),
        }),
    };

    assert_eq!(actual, expected);
}

// https://www.w3.org/TR/mediacapture-streams/#example-3
#[cfg(feature = "serde")]
#[test]
fn w3c_spec_example_3() {
    let actual: MediaStreamConstraints = {
        let json = serde_json::json!({
          "video": {
              "height": { "min": 480, "ideal": 720 },
              "width": { "min": 640, "ideal": 1280 },
              "frameRate": { "min": 30.0 },
            "advanced": [
              {"width": 1920, "height": 1280 },
              {"aspectRatio": 1.333},
              {"frameRate": {"min": 50.0 } },
              {"frameRate": {"min": 40.0 } }
            ]
          }
        });
        serde_json::from_value(json).unwrap()
    };

    let expected = MediaStreamConstraints {
        audio: BoolOrMediaTrackConstraints::Bool(false),
        video: BoolOrMediaTrackConstraints::Constraints(MediaTrackConstraints {
            basic: BareOrMediaTrackConstraintSet::from_iter([
                (
                    HEIGHT,
                    ValueRangeConstraint {
                        min: Some(480),
                        max: None,
                        exact: None,
                        ideal: Some(720),
                    }
                    .into(),
                ),
                (
                    WIDTH,
                    ValueRangeConstraint {
                        min: Some(640),
                        max: None,
                        exact: None,
                        ideal: Some(1280),
                    }
                    .into(),
                ),
                (
                    FRAME_RATE,
                    ValueRangeConstraint {
                        min: Some(30.0),
                        max: None,
                        exact: None,
                        ideal: None,
                    }
                    .into(),
                ),
            ]),
            advanced: AdvancedMediaTrackConstraints::new(vec![
                BareOrMediaTrackConstraintSet::from_iter([
                    (WIDTH, 1920.into()),
                    (HEIGHT, 1280.into()),
                ]),
                BareOrMediaTrackConstraintSet::from_iter([(ASPECT_RATIO, 1.333.into())]),
                BareOrMediaTrackConstraintSet::from_iter([(
                    FRAME_RATE,
                    ValueRangeConstraint {
                        min: Some(50.0),
                        max: None,
                        exact: None,
                        ideal: None,
                    }
                    .into(),
                )]),
                BareOrMediaTrackConstraintSet::from_iter([(
                    FRAME_RATE,
                    ValueRangeConstraint {
                        min: Some(40.0),
                        max: None,
                        exact: None,
                        ideal: None,
                    }
                    .into(),
                )]),
            ]),
        }),
    };

    assert_eq!(actual, expected);
}

// https://www.w3.org/TR/mediacapture-streams/#example-4
#[cfg(feature = "serde")]
#[test]
fn w3c_spec_example_4() {
    let actual: BareOrMediaTrackConstraintSet = {
        let json = serde_json::json!({
            "width": 1920,
            "height": 1080,
            "frameRate": 30,
        });
        serde_json::from_value(json).unwrap()
    };

    let expected = BareOrMediaTrackConstraintSet::from_iter([
        (WIDTH, BareOrValueRangeConstraint::Bare(1920).into()),
        (HEIGHT, BareOrValueRangeConstraint::Bare(1080).into()),
        (FRAME_RATE, BareOrValueRangeConstraint::Bare(30).into()),
    ]);

    assert_eq!(actual, expected);
}
