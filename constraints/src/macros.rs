#[macro_export]
macro_rules! settings {
    [
        $($p:expr => $c:expr),* $(,)?
    ] => {
        $crate::MediaTrackSettings::from_iter([
            $(($p, $c.into())),*
        ])
    };
}

#[macro_export]
macro_rules! bare_or_constraint {
    (value: {
        $($p:ident: $c:expr),+ $(,)?
    }) => {
        bare_or_value_constraint!{
            $($p: $c),+
        }
    };
    (value: $c:expr) => {
        bare_or_value_constraint! {
            $c
        }
    };
    (range: {
        $($p:ident: $c:expr),+ $(,)?
    }) => {
        bare_or_value_range_constraint!{
            $($p: $c),+
        }
    };
    (range: $c:expr) => {
        bare_or_value_range_constraint! {
            $c
        }
    };
    (sequence: {
        $($p:ident: $c:expr),+ $(,)?
    }) => {
        bare_or_value_sequence_constraint!{
            $($p: $c),+
        }
    };
    (sequence: $c:expr) => {
        bare_or_value_sequence_constraint! {
            $c
        }
    };
}

#[macro_export]
macro_rules! resolved_constraint {
    (value: {
        $($p:ident: $c:expr),* $(,)?
    }) => {
        $crate::ResolvedMediaTrackConstraint::from(value_constraint!{
            $($p: $c),*
        })
    };
    (range: {
        $($p:ident: $c:expr),* $(,)?
    }) => {
        $crate::ResolvedMediaTrackConstraint::from(value_range_constraint!{
            $($p: $c),*
        })
    };
    (sequence: {
        $($p:ident: $c:expr),* $(,)?
    }) => {
        $crate::ResolvedMediaTrackConstraint::from(value_sequence_constraint!{
            $($p: $c),*
        })
    };
}

#[macro_export]
macro_rules! sanitized_constraint {
    (value: {
        $($p:ident: $c:expr),+ $(,)?
    }) => {
        resolved_constraint!(value: {
            $($p: $c),+
        }).into_sanitized().unwrap()
    };
    (range: {
        $($p:ident: $c:expr),+ $(,)?
    }) => {
        resolved_constraint!(range: {
            $($p: $c),+
        }).into_sanitized().unwrap()
    };
    (sequence: {
        $($p:ident: $c:expr),+ $(,)?
    }) => {
        resolved_constraint!(sequence: {
            $($p: $c),+
        }).into_sanitized().unwrap()
    };
}

#[macro_export]
macro_rules! value_constraint {
    {
        $($p:ident: $c:expr),+ $(,)?
    } => {
        {
            #[allow(clippy::needless_update)]
            let constraint = $crate::ResolvedValueConstraint {
                $($p: Some($c)),*,
                ..Default::default()
            };
            constraint
        }
    };
}

#[macro_export]
macro_rules! bare_or_value_constraint {
    {
        $($p:ident: $c:expr),+ $(,)?
    } => {
        $crate::BareOrValueConstraint::Constraint(value_constraint! {
            $($p: $c),+
        })
    };
    {
        $b:expr
    } => {
        $crate::BareOrValueConstraint::Bare($b)
    };
}

#[macro_export]
macro_rules! value_range_constraint {
    {
        $($p:ident: $c:expr),+ $(,)?
    } => {
        {
            #[allow(clippy::needless_update)]
            let constraint = $crate::ResolvedValueRangeConstraint {
                $($p: Some($c)),*,
                ..Default::default()
            };
            constraint
        }
    };
}

#[macro_export]
macro_rules! bare_or_value_range_constraint {
    {
        $($p:ident: $c:expr),+ $(,)?
    } => {
        $crate::BareOrValueRangeConstraint::Constraint(value_range_constraint! {
            $($p: $c),+
        })
    };
    {
        $b:expr
    } => {
        $crate::BareOrValueRangeConstraint::Bare($b)
    };
}

#[macro_export]
macro_rules! value_sequence_constraint {
    {
        $($p:ident: $c:expr),+ $(,)?
    } => {
        {
            #[allow(clippy::needless_update)]
            $crate::ResolvedValueSequenceConstraint {
                $($p: Some($c)),*,
                ..Default::default()
            }
        }
    };
}

#[macro_export]
macro_rules! bare_or_value_sequence_constraint {
    {
        $($p:ident: $c:expr),+ $(,)?
    } => {
        $crate::BareOrValueSequenceConstraint::Constraint(value_sequence_constraint! {
            $($p: $c),+
        })
    };
    {
        $b:expr
    } => {
        $crate::BareOrValueSequenceConstraint::Bare($b)
    };
}

#[macro_export]
macro_rules! constraint_set {
    {
        $($p:expr => $c:expr),* $(,)?
    } => {
        $crate::ResolvedMediaTrackConstraintSet::from_iter([
            $(($p, $c.into())),*
        ])
    };
}

#[macro_export]
macro_rules! mandatory_constraints {
    {
        $($p:expr => $c:expr),* $(,)?
    } => {
        $crate::ResolvedMandatoryMediaTrackConstraints::new(
            constraint_set!{
                $($p => $c),*
            }
        )
    };
}

#[macro_export]
macro_rules! advanced_constraints {
    [
        $({
            $($p:expr => $c:expr),* $(,)?
        }),* $(,)?
    ] => {
        $crate::ResolvedAdvancedMediaTrackConstraints::from_iter([
            $(constraint_set!{
                $($p => $c),*
            }),*
        ])
    };
}

#[macro_export]
macro_rules! constraints {
    [
        mandatory: {$($mp:expr => $mc:expr),* $(,)?},
        advanced: [$(
            {$($ap:expr => $ac:expr),* $(,)?}
        ),* $(,)?]
    ] => {
        $crate::ResolvedMediaTrackConstraints {
            mandatory: mandatory_constraints!($($mp => $mc),*),
            advanced: advanced_constraints!($({ $($ap => $ac),* }),*)
        }
    };
}

#[cfg(test)]
macro_rules! test_serde_symmetry {
    (subject: $s:expr, json: $j:expr) => {
        // Serialize:
        {
            let actual = serde_json::to_value($s.clone()).unwrap();
            let expected = $j.clone();

            assert_eq!(actual, expected);
        }

        // Deserialize:
        {
            let actual: Subject = serde_json::from_value($j).unwrap();
            let expected = $s;

            assert_eq!(actual, expected);
        }
    };
}

#[cfg(test)]
pub(crate) use test_serde_symmetry;

#[cfg(test)]
mod tests {
    use crate::{
        property::all::name::*, BareOrMediaTrackConstraint, BareOrValueConstraint,
        BareOrValueRangeConstraint, BareOrValueSequenceConstraint, FacingMode, MediaTrackSettings,
        ResolvedAdvancedMediaTrackConstraints, ResolvedMandatoryMediaTrackConstraints,
        ResolvedMediaTrackConstraint, ResolvedMediaTrackConstraintSet,
        ResolvedMediaTrackConstraints, SanitizedMediaTrackConstraint, ResolvedValueConstraint,
        ResolvedValueRangeConstraint, ResolvedValueSequenceConstraint,
    };

    #[test]
    fn settings() {
        let actual: MediaTrackSettings = settings![
            DEVICE_ID => "foobar".to_owned(),
            FRAME_RATE => 30.0,
            HEIGHT => 1080,
            FACING_MODE => FacingMode::user(),
        ];

        let expected = MediaTrackSettings::from_iter([
            (DEVICE_ID, "foobar".to_owned().into()),
            (FRAME_RATE, 30.0.into()),
            (HEIGHT, 1080.into()),
            (FACING_MODE, FacingMode::user().into()),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn value_constraint() {
        let actual = value_constraint! {
            exact: "foobar".to_owned(),
            ideal: "bazblee".to_owned(),
        };

        let expected = ResolvedValueConstraint::default()
            .exact("foobar".to_owned())
            .ideal("bazblee".to_owned());

        assert_eq!(actual, expected);
    }

    mod bare_or_constraint {
        use super::*;

        #[test]
        fn value() {
            // Bare:

            let actual = bare_or_constraint!(value: "foobar".to_owned());

            let expected = BareOrValueConstraint::Bare("foobar".to_owned());

            assert_eq!(actual, expected);

            // Constraint:

            let actual = bare_or_constraint!(value: {
                exact: "foobar".to_owned(),
                ideal: "bazblee".to_owned(),
            });

            let expected = BareOrValueConstraint::Constraint(
                ResolvedValueConstraint::default()
                    .exact("foobar".to_owned())
                    .ideal("bazblee".to_owned()),
            );

            assert_eq!(actual, expected);
        }

        #[test]
        fn range() {
            // Bare:

            let actual = bare_or_constraint!(range: 42);

            let expected = BareOrValueRangeConstraint::Bare(42);

            assert_eq!(actual, expected);

            // Constraint:

            let actual = bare_or_constraint!(range: {
                min: 30.0,
                max: 60.0,
            });

            let expected = BareOrValueRangeConstraint::Constraint(
                ResolvedValueRangeConstraint::default().min(30.0).max(60.0),
            );

            assert_eq!(actual, expected);
        }

        #[test]
        fn sequence() {
            // Bare:

            let actual = bare_or_constraint!(sequence: vec![FacingMode::user()]);

            let expected = BareOrValueSequenceConstraint::Bare(vec![FacingMode::user()]);

            assert_eq!(actual, expected);

            // Constraint:

            let actual = bare_or_constraint!(sequence: {
                ideal: vec![FacingMode::user()],
            });

            let expected = BareOrValueSequenceConstraint::Constraint(
                ResolvedValueSequenceConstraint::default().ideal(vec![FacingMode::user()]),
            );

            assert_eq!(actual, expected);
        }
    }

    mod constraint {
        use super::*;

        #[test]
        fn value() {
            let actual = resolved_constraint!(value: {
                exact: "foobar".to_owned(),
                ideal: "bazblee".to_owned(),
            });

            let expected = ResolvedMediaTrackConstraint::from(
                ResolvedValueConstraint::default()
                    .exact("foobar".to_owned())
                    .ideal("bazblee".to_owned()),
            );

            assert_eq!(actual, expected);
        }

        #[test]
        fn range() {
            let actual = resolved_constraint!(range: {
                min: 30.0,
                max: 60.0,
            });

            let expected = ResolvedMediaTrackConstraint::from(
                ResolvedValueRangeConstraint::default().min(30.0).max(60.0),
            );

            assert_eq!(actual, expected);
        }

        #[test]
        fn sequence() {
            let actual = resolved_constraint!(sequence: {
                ideal: vec![FacingMode::user()],
            });

            let expected = ResolvedMediaTrackConstraint::from(
                ResolvedValueSequenceConstraint::default().ideal(vec![FacingMode::user()]),
            );

            assert_eq!(actual, expected);
        }
    }

    mod sanitized_constraint {
        use super::*;

        #[test]
        fn value() {
            let actual = sanitized_constraint!(value: {
                exact: "foobar".to_owned(),
                ideal: "bazblee".to_owned(),
            });

            let expected = ResolvedMediaTrackConstraint::from(
                ResolvedValueConstraint::default()
                    .exact("foobar".to_owned())
                    .ideal("bazblee".to_owned()),
            )
            .into_sanitized()
            .unwrap();

            assert_eq!(actual, expected);
        }

        #[test]
        fn range() {
            let actual = sanitized_constraint!(range: {
                min: 30.0,
                max: 60.0,
            });

            let expected = ResolvedMediaTrackConstraint::from(
                ResolvedValueRangeConstraint::default().min(30.0).max(60.0),
            )
            .into_sanitized()
            .unwrap();

            assert_eq!(actual, expected);
        }

        #[test]
        fn sequence() {
            let actual = sanitized_constraint!(sequence: {
                ideal: vec![FacingMode::user()],
            });

            let expected = ResolvedMediaTrackConstraint::from(
                ResolvedValueSequenceConstraint::default().ideal(vec![FacingMode::user()]),
            )
            .into_sanitized()
            .unwrap();

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn value_range_constraint() {
        let actual = value_range_constraint! {
            min: 30.0,
            max: 60.0,
        };

        let expected = ResolvedValueRangeConstraint::default().min(30.0).max(60.0);

        assert_eq!(actual, expected);
    }

    #[test]
    fn value_sequence_constraint() {
        let actual = value_sequence_constraint! {
            exact: vec![FacingMode::user(), FacingMode::environment()]
        };
        let expected = ResolvedValueSequenceConstraint::default()
            .exact(vec![FacingMode::user(), FacingMode::environment()]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn mandatory_constraints() {
        let actual = mandatory_constraints! {
            DEVICE_ID => resolved_constraint!(value: {
                exact: "foobar".to_owned(),
                ideal: "bazblee".to_owned(),
            }),
            FRAME_RATE => resolved_constraint!(range: {
                min: 30.0,
                max: 60.0,
            }),
            FACING_MODE => resolved_constraint!(sequence: {
                exact: vec![FacingMode::user(), FacingMode::environment()]
            }),
        };

        let expected = ResolvedMandatoryMediaTrackConstraints::from_iter([
            (
                DEVICE_ID,
                ResolvedValueConstraint::default()
                    .exact("foobar".to_owned())
                    .ideal("bazblee".to_owned())
                    .into(),
            ),
            (
                FRAME_RATE,
                ResolvedValueRangeConstraint::default().min(30.0).max(60.0).into(),
            ),
            (
                FACING_MODE,
                ResolvedValueSequenceConstraint::default()
                    .exact(vec![FacingMode::user(), FacingMode::environment()])
                    .into(),
            ),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn advanced_constraints() {
        let actual = advanced_constraints! [
            {
                DEVICE_ID => resolved_constraint!(value: {
                    exact: "foobar".to_owned(),
                    ideal: "bazblee".to_owned(),
                }),
            },
            {
                FRAME_RATE => resolved_constraint!(range: {
                    min: 30.0,
                    max: 60.0,
                }),
            },
            {
                FACING_MODE => resolved_constraint!(sequence: {
                    exact: vec![FacingMode::user(), FacingMode::environment()]
                }),
            },
        ];

        let expected = ResolvedAdvancedMediaTrackConstraints::from_iter([
            ResolvedMediaTrackConstraintSet::from_iter([(
                DEVICE_ID,
                ResolvedValueConstraint::default()
                    .exact("foobar".to_owned())
                    .ideal("bazblee".to_owned())
                    .into(),
            )]),
            ResolvedMediaTrackConstraintSet::from_iter([(
                FRAME_RATE,
                ResolvedValueRangeConstraint::default().min(30.0).max(60.0).into(),
            )]),
            ResolvedMediaTrackConstraintSet::from_iter([(
                FACING_MODE,
                ResolvedValueSequenceConstraint::default()
                    .exact(vec![FacingMode::user(), FacingMode::environment()])
                    .into(),
            )]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn constraints() {
        let actual: ResolvedMediaTrackConstraints = constraints!(
            mandatory: {
                DEVICE_ID => resolved_constraint!(value: {
                    exact: "foobar".to_owned(),
                    ideal: "bazblee".to_owned(),
                }),
                FRAME_RATE => resolved_constraint!(range: {
                    min: 30.0,
                    max: 60.0,
                }),
                FACING_MODE => resolved_constraint!(sequence: {
                    exact: vec![FacingMode::user(), FacingMode::environment()]
                }),
            },
            advanced: [
                {
                    DEVICE_ID => resolved_constraint!(value: {
                        exact: "foobar".to_owned(),
                        ideal: "bazblee".to_owned(),
                    }),
                },
                {
                    FRAME_RATE => resolved_constraint!(range: {
                        min: 30.0,
                        max: 60.0,
                    }),
                },
                {
                    FACING_MODE => resolved_constraint!(sequence: {
                        exact: vec![FacingMode::user(), FacingMode::environment()]
                    }),
                },
            ]
        );

        let expected = ResolvedMediaTrackConstraints {
            mandatory: ResolvedMandatoryMediaTrackConstraints::from_iter([
                (
                    DEVICE_ID,
                    ResolvedValueConstraint::default()
                        .exact("foobar".to_owned())
                        .ideal("bazblee".to_owned())
                        .into(),
                ),
                (
                    FRAME_RATE,
                    ResolvedValueRangeConstraint::default().min(30.0).max(60.0).into(),
                ),
                (
                    FACING_MODE,
                    ResolvedValueSequenceConstraint::default()
                        .exact(vec![FacingMode::user(), FacingMode::environment()])
                        .into(),
                ),
            ]),
            advanced: ResolvedAdvancedMediaTrackConstraints::from_iter([
                ResolvedMediaTrackConstraintSet::from_iter([(
                    DEVICE_ID,
                    ResolvedValueConstraint::default()
                        .exact("foobar".to_owned())
                        .ideal("bazblee".to_owned())
                        .into(),
                )]),
                ResolvedMediaTrackConstraintSet::from_iter([(
                    FRAME_RATE,
                    ResolvedValueRangeConstraint::default().min(30.0).max(60.0).into(),
                )]),
                ResolvedMediaTrackConstraintSet::from_iter([(
                    FACING_MODE,
                    ResolvedValueSequenceConstraint::default()
                        .exact(vec![FacingMode::user(), FacingMode::environment()])
                        .into(),
                )]),
            ]),
        };

        assert_eq!(actual, expected);
    }
}
