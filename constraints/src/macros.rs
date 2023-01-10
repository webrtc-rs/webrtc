//! Convenience macros.

/// A convenience macro for defining settings.
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

pub use settings;

/// A convenience macro for defining individual "value" constraints.
#[macro_export]
macro_rules! value_constraint {
    ($($p:ident: $c:expr),+ $(,)?) => {
        $crate::ValueConstraint::Constraint(
            #[allow(clippy::needless_update)]
            $crate::ResolvedValueConstraint {
                $($p: Some($c)),+,
                ..Default::default()
            }
        )
    };
    ($c:expr) => {
        $crate::ValueConstraint::Bare($c)
    };
}

pub use value_constraint;

/// A convenience macro for defining individual "value range" constraints.
#[macro_export]
macro_rules! value_range_constraint {
    {$($p:ident: $c:expr),+ $(,)?} => {
        $crate::ValueRangeConstraint::Constraint(
            $crate::ResolvedValueRangeConstraint {
                $($p: Some($c)),+,
                ..Default::default()
            }
        )
    };
    ($c:expr) => {
        $crate::ValueRangeConstraint::Bare($c)
    };
}

pub use value_range_constraint;

/// A convenience macro for defining individual "value sequence" constraints.
#[macro_export]
macro_rules! value_sequence_constraint {
    {$($p:ident: $c:expr),+ $(,)?} => {
        $crate::ValueSequenceConstraint::Constraint(
            $crate::ResolvedValueSequenceConstraint {
                $($p: Some($c)),*,
                ..Default::default()
            }
        )
    };
    ($c:expr) => {
        $crate::ValueSequenceConstraint::Bare($c)
    };
}

pub use value_sequence_constraint;

/// A convenience macro for defining constraint sets.
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

pub use constraint_set;

/// A convenience macro for defining "mandatory" constraints.
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

pub use mandatory_constraints;

/// A convenience macro for defining "advanced" constraints.
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

pub use advanced_constraints;

/// A convenience macro for defining constraints.
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

pub use constraints;

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
            &DEVICE_ID => "foobar".to_owned(),
            &FRAME_RATE => 30.0,
            &HEIGHT => 1080,
            &FACING_MODE => FacingMode::user(),
        ];

        let expected = <MediaTrackSettings as std::iter::FromIterator<_>>::from_iter([
            (&DEVICE_ID, "foobar".to_owned().into()),
            (&FRAME_RATE, 30.0.into()),
            (&HEIGHT, 1080.into()),
            (&FACING_MODE, FacingMode::user().into()),
        ]);

        assert_eq!(actual, expected);
    }

    mod constraint {
        use super::*;

        #[test]
        fn value() {
            // Bare:

            let actual = value_constraint!("foobar".to_owned());

            let expected = ValueConstraint::Bare("foobar".to_owned());

            assert_eq!(actual, expected);

            // Constraint:

            let actual = value_constraint! {
                exact: "foobar".to_owned(),
                ideal: "bazblee".to_owned(),
            };

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

            let actual = value_range_constraint!(42);

            let expected = ValueRangeConstraint::Bare(42);

            assert_eq!(actual, expected);

            // Constraint:

            let actual = value_range_constraint! {
                min: 30.0,
                max: 60.0,
            };

            let expected = ValueRangeConstraint::Constraint(
                ResolvedValueRangeConstraint::default().min(30.0).max(60.0),
            );

            assert_eq!(actual, expected);
        }

        #[test]
        fn sequence() {
            // Bare:

            let actual = value_sequence_constraint![vec![FacingMode::user()]];

            let expected = ValueSequenceConstraint::Bare(vec![FacingMode::user()]);

            assert_eq!(actual, expected);

            // Constraint:

            let actual = value_sequence_constraint! {
                ideal: vec![FacingMode::user()],
            };

            let expected = ValueSequenceConstraint::Constraint(
                ResolvedValueSequenceConstraint::default().ideal(vec![FacingMode::user()]),
            );

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn mandatory_constraints() {
        let actual = mandatory_constraints! {
            &DEVICE_ID => value_constraint! {
                exact: "foobar".to_owned(),
                ideal: "bazblee".to_owned(),
            },
            &FRAME_RATE => value_range_constraint! {
                min: 30.0,
                max: 60.0,
            },
            &FACING_MODE => value_sequence_constraint! {
                exact: vec![FacingMode::user(), FacingMode::environment()]
            },
        };

        let expected = <MandatoryMediaTrackConstraints as std::iter::FromIterator<_>>::from_iter([
            (
                &DEVICE_ID,
                ValueConstraint::Constraint(
                    ResolvedValueConstraint::default()
                        .exact("foobar".to_owned())
                        .ideal("bazblee".to_owned()),
                )
                .into(),
            ),
            (
                &FRAME_RATE,
                ValueRangeConstraint::Constraint(
                    ResolvedValueRangeConstraint::default().min(30.0).max(60.0),
                )
                .into(),
            ),
            (
                &FACING_MODE,
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
                &DEVICE_ID => value_constraint! {
                    exact: "foobar".to_owned(),
                    ideal: "bazblee".to_owned(),
                },
            },
            {
                &FRAME_RATE => value_range_constraint! {
                    min: 30.0,
                    max: 60.0,
                },
            },
            {
                &FACING_MODE => value_sequence_constraint! {
                    exact: vec![FacingMode::user(), FacingMode::environment()]
                },
            },
        ];

        let expected = <AdvancedMediaTrackConstraints as std::iter::FromIterator<_>>::from_iter([
            <MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([(
                &DEVICE_ID,
                ResolvedValueConstraint::default()
                    .exact("foobar".to_owned())
                    .ideal("bazblee".to_owned())
                    .into(),
            )]),
            <MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([(
                &FRAME_RATE,
                ResolvedValueRangeConstraint::default()
                    .min(30.0)
                    .max(60.0)
                    .into(),
            )]),
            <MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([(
                &FACING_MODE,
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
                &DEVICE_ID => value_constraint! {
                    exact: "foobar".to_owned(),
                    ideal: "bazblee".to_owned(),
                },
                &FRAME_RATE => value_range_constraint! {
                    min: 30.0,
                    max: 60.0,
                },
                &FACING_MODE => value_sequence_constraint! {
                    exact: vec![FacingMode::user(), FacingMode::environment()]
                },
            },
            advanced: [
                {
                    &DEVICE_ID => value_constraint! {
                        exact: "foobar".to_owned(),
                        ideal: "bazblee".to_owned(),
                    },
                },
                {
                    &FRAME_RATE => value_range_constraint! {
                        min: 30.0,
                        max: 60.0,
                    },
                },
                {
                    &FACING_MODE => value_sequence_constraint! {
                        exact: vec![FacingMode::user(), FacingMode::environment()]
                    },
                },
            ]
        );

        let expected = MediaTrackConstraints {
            mandatory: <MandatoryMediaTrackConstraints as std::iter::FromIterator<_>>::from_iter([
                (
                    &DEVICE_ID,
                    ResolvedValueConstraint::default()
                        .exact("foobar".to_owned())
                        .ideal("bazblee".to_owned())
                        .into(),
                ),
                (
                    &FRAME_RATE,
                    ResolvedValueRangeConstraint::default()
                        .min(30.0)
                        .max(60.0)
                        .into(),
                ),
                (
                    &FACING_MODE,
                    ResolvedValueSequenceConstraint::default()
                        .exact(vec![FacingMode::user(), FacingMode::environment()])
                        .into(),
                ),
            ]),
            advanced: <AdvancedMediaTrackConstraints as std::iter::FromIterator<_>>::from_iter([
                <MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([(
                    &DEVICE_ID,
                    ResolvedValueConstraint::default()
                        .exact("foobar".to_owned())
                        .ideal("bazblee".to_owned())
                        .into(),
                )]),
                <MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([(
                    &FRAME_RATE,
                    ResolvedValueRangeConstraint::default()
                        .min(30.0)
                        .max(60.0)
                        .into(),
                )]),
                <MediaTrackConstraintSet as std::iter::FromIterator<_>>::from_iter([(
                    &FACING_MODE,
                    ResolvedValueSequenceConstraint::default()
                        .exact(vec![FacingMode::user(), FacingMode::environment()])
                        .into(),
                )]),
            ]),
        };

        assert_eq!(actual, expected);
    }
}
