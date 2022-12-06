#[macro_export]
macro_rules! settings {
    [
        $($p:expr => $c:expr),* $(,)?
    ] => {
        <$crate::MediaTrackSettings as std::iter::FromIterator<_>>::from_iter([
            $(($p, $c.into())),*
        ])
    };
}

#[macro_export]
macro_rules! constraint {
    (value: {
        $($p:ident: $c:expr),+ $(,)?
    }) => {
        $crate::ValueConstraint::Constraint(
            #[allow(clippy::needless_update)]
            $crate::ResolvedValueConstraint {
                $($p: Some($c)),+,
                ..Default::default()
            }
        )
    };
    (value: $c:expr) => {
        $crate::ValueConstraint::Bare($c)
    };
    (range: {
        $($p:ident: $c:expr),+ $(,)?
    }) => {
        $crate::ValueRangeConstraint::Constraint(
            $crate::ResolvedValueRangeConstraint {
                $($p: Some($c)),+,
                ..Default::default()
            }
        )
    };
    (range: $c:expr) => {
        $crate::ValueRangeConstraint::Bare($c)
    };
    (sequence: {
        $($p:ident: $c:expr),+ $(,)?
    }) => {
        $crate::ValueSequenceConstraint::Constraint(
            $crate::ResolvedValueSequenceConstraint {
                $($p: Some($c)),*,
                ..Default::default()
            }
        )
    };
    (sequence: $c:expr) => {
        $crate::ValueSequenceConstraint::Bare($c)
    };
}

#[macro_export]
macro_rules! constraint_set {
    {
        $($p:expr => $c:expr),* $(,)?
    } => {
        <$crate::MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([
            $(($p, $c.into())),*
        ])
    };
}

#[macro_export]
macro_rules! mandatory_constraints {
    {
        $($p:expr => $c:expr),* $(,)?
    } => {
        $crate::MandatoryMediaTrackConstraints::new(
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
        <$crate::AdvancedMediaTrackConstraints as std::iter::FromIterator<_>>::from_iter([
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
        $crate::MediaTrackConstraints {
            mandatory: mandatory_constraints!($($mp => $mc),*),
            advanced: advanced_constraints!($({ $($ap => $ac),* }),*)
        }
    };
}

#[allow(unused_macros)]
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

#[allow(unused_imports)]
#[cfg(test)]
pub(crate) use test_serde_symmetry;

#[cfg(test)]
mod tests {
    use crate::{
        property::all::name::*, AdvancedMediaTrackConstraints, FacingMode,
        MandatoryMediaTrackConstraints, MediaTrackConstraintSet, MediaTrackConstraints,
        MediaTrackSettings, ResolvedValueConstraint, ResolvedValueRangeConstraint,
        ResolvedValueSequenceConstraint, ValueConstraint, ValueRangeConstraint,
        ValueSequenceConstraint,
    };

    #[test]
    fn settings() {
        let actual: MediaTrackSettings = settings![
            DEVICE_ID => "foobar".to_owned(),
            FRAME_RATE => 30.0,
            HEIGHT => 1080,
            FACING_MODE => FacingMode::user(),
        ];

        let expected = <MediaTrackSettings as std::iter::FromIterator<_>>::from_iter([
            (DEVICE_ID, "foobar".to_owned().into()),
            (FRAME_RATE, 30.0.into()),
            (HEIGHT, 1080.into()),
            (FACING_MODE, FacingMode::user().into()),
        ]);

        assert_eq!(actual, expected);
    }

    mod constraint {
        use super::*;

        #[test]
        fn value() {
            // Bare:

            let actual = constraint!(value: "foobar".to_owned());

            let expected = ValueConstraint::Bare("foobar".to_owned());

            assert_eq!(actual, expected);

            // Constraint:

            let actual = constraint!(value: {
                exact: "foobar".to_owned(),
                ideal: "bazblee".to_owned(),
            });

            let expected = ValueConstraint::Constraint(
                ResolvedValueConstraint::default()
                    .exact("foobar".to_owned())
                    .ideal("bazblee".to_owned()),
            );

            assert_eq!(actual, expected);
        }

        #[test]
        fn range() {
            // Bare:

            let actual = constraint!(range: 42);

            let expected = ValueRangeConstraint::Bare(42);

            assert_eq!(actual, expected);

            // Constraint:

            let actual = constraint!(range: {
                min: 30.0,
                max: 60.0,
            });

            let expected = ValueRangeConstraint::Constraint(
                ResolvedValueRangeConstraint::default().min(30.0).max(60.0),
            );

            assert_eq!(actual, expected);
        }

        #[test]
        fn sequence() {
            // Bare:

            let actual = constraint!(sequence: vec![FacingMode::user()]);

            let expected = ValueSequenceConstraint::Bare(vec![FacingMode::user()]);

            assert_eq!(actual, expected);

            // Constraint:

            let actual = constraint!(sequence: {
                ideal: vec![FacingMode::user()],
            });

            let expected = ValueSequenceConstraint::Constraint(
                ResolvedValueSequenceConstraint::default().ideal(vec![FacingMode::user()]),
            );

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn mandatory_constraints() {
        let actual = mandatory_constraints! {
            DEVICE_ID => constraint!(value: {
                exact: "foobar".to_owned(),
                ideal: "bazblee".to_owned(),
            }),
            FRAME_RATE => constraint!(range: {
                min: 30.0,
                max: 60.0,
            }),
            FACING_MODE => constraint!(sequence: {
                exact: vec![FacingMode::user(), FacingMode::environment()]
            }),
        };

        let expected = <MandatoryMediaTrackConstraints as std::iter::FromIterator<_>>::from_iter([
            (
                DEVICE_ID,
                ValueConstraint::Constraint(
                    ResolvedValueConstraint::default()
                        .exact("foobar".to_owned())
                        .ideal("bazblee".to_owned()),
                )
                .into(),
            ),
            (
                FRAME_RATE,
                ValueRangeConstraint::Constraint(
                    ResolvedValueRangeConstraint::default().min(30.0).max(60.0),
                )
                .into(),
            ),
            (
                FACING_MODE,
                ValueSequenceConstraint::Constraint(
                    ResolvedValueSequenceConstraint::default()
                        .exact(vec![FacingMode::user(), FacingMode::environment()]),
                )
                .into(),
            ),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn advanced_constraints() {
        let actual = advanced_constraints! [
            {
                DEVICE_ID => constraint!(value: {
                    exact: "foobar".to_owned(),
                    ideal: "bazblee".to_owned(),
                }),
            },
            {
                FRAME_RATE => constraint!(range: {
                    min: 30.0,
                    max: 60.0,
                }),
            },
            {
                FACING_MODE => constraint!(sequence: {
                    exact: vec![FacingMode::user(), FacingMode::environment()]
                }),
            },
        ];

        let expected = <AdvancedMediaTrackConstraints as std::iter::FromIterator<_>>::from_iter([
            <MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([(
                DEVICE_ID,
                ResolvedValueConstraint::default()
                    .exact("foobar".to_owned())
                    .ideal("bazblee".to_owned())
                    .into(),
            )]),
            <MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([(
                FRAME_RATE,
                ResolvedValueRangeConstraint::default()
                    .min(30.0)
                    .max(60.0)
                    .into(),
            )]),
            <MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([(
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
        let actual: MediaTrackConstraints = constraints!(
            mandatory: {
                DEVICE_ID => constraint!(value: {
                    exact: "foobar".to_owned(),
                    ideal: "bazblee".to_owned(),
                }),
                FRAME_RATE => constraint!(range: {
                    min: 30.0,
                    max: 60.0,
                }),
                FACING_MODE => constraint!(sequence: {
                    exact: vec![FacingMode::user(), FacingMode::environment()]
                }),
            },
            advanced: [
                {
                    DEVICE_ID => constraint!(value: {
                        exact: "foobar".to_owned(),
                        ideal: "bazblee".to_owned(),
                    }),
                },
                {
                    FRAME_RATE => constraint!(range: {
                        min: 30.0,
                        max: 60.0,
                    }),
                },
                {
                    FACING_MODE => constraint!(sequence: {
                        exact: vec![FacingMode::user(), FacingMode::environment()]
                    }),
                },
            ]
        );

        let expected = MediaTrackConstraints {
            mandatory: <MandatoryMediaTrackConstraints as std::iter::FromIterator<_>>::from_iter([
                (
                    DEVICE_ID,
                    ResolvedValueConstraint::default()
                        .exact("foobar".to_owned())
                        .ideal("bazblee".to_owned())
                        .into(),
                ),
                (
                    FRAME_RATE,
                    ResolvedValueRangeConstraint::default()
                        .min(30.0)
                        .max(60.0)
                        .into(),
                ),
                (
                    FACING_MODE,
                    ResolvedValueSequenceConstraint::default()
                        .exact(vec![FacingMode::user(), FacingMode::environment()])
                        .into(),
                ),
            ]),
            advanced: <AdvancedMediaTrackConstraints as std::iter::FromIterator<_>>::from_iter([
                <MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([(
                    DEVICE_ID,
                    ResolvedValueConstraint::default()
                        .exact("foobar".to_owned())
                        .ideal("bazblee".to_owned())
                        .into(),
                )]),
                <MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([(
                    FRAME_RATE,
                    ResolvedValueRangeConstraint::default()
                        .min(30.0)
                        .max(60.0)
                        .into(),
                )]),
                <MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([(
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
