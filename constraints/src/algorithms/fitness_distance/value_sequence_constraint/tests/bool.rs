use crate::algorithms::SettingFitnessDistanceErrorKind;

use super::*;

mod basic {
    use super::*;

    mod zero_distance {
        use super::*;

        generate_value_constraint_tests!(
            tests: [
                {
                    name: bool_setting,
                    settings: bool => &[Some(true)],
                },
            ],
            constraints: bool => &[
                ResolvedValueSequenceConstraint {
                    exact: None,
                    ideal: None,
                },
                ResolvedValueSequenceConstraint {
                    exact: None,
                    ideal: Some(vec![true]),
                },
            ],
            expected: Ok(0.0)
        );
    }

    mod one_distance {
        use super::*;

        generate_value_constraint_tests!(
            tests: [
                {
                    name: bool_setting,
                    settings: bool => &[None, Some(false)],
                },
            ],
            constraints: bool => &[ResolvedValueSequenceConstraint {
                exact: None,
                ideal: Some(vec![true]),
            }],
            expected: Ok(1.0)
        );
    }
}

mod required {
    use super::*;

    mod zero_distance {
        use super::*;
        // A constraint that does apply for a type of setting,
        // is expected to return a fitness distance of `0`,
        // iff the setting matches the constraint:
        generate_value_constraint_tests!(
            tests: [
                {
                    name: bool_setting,
                    settings: bool => &[Some(true)],
                },
            ],
            constraints: bool => &[ResolvedValueSequenceConstraint {
                exact: Some(vec![true]),
                ideal: None,
            }],
            expected: Ok(0.0)
        );
    }

    mod inf_distance {
        use super::*;

        mod missing {
            use super::*;

            generate_value_constraint_tests!(
                tests: [
                    {
                        name: bool_setting,
                        settings: bool => &[None],
                    },
                ],
                constraints: bool => &[
                    ResolvedValueSequenceConstraint {
                        exact: Some(vec![true]),
                        ideal: None,
                    },
                    ResolvedValueSequenceConstraint {
                        exact: Some(vec![true]),
                        ideal: Some(vec![true]),
                    },
                ],
                expected: Err(SettingFitnessDistanceError {
                    kind: SettingFitnessDistanceErrorKind::Missing,
                    constraint: "(x == [true])".to_owned(),
                    setting: None,
                })
            );
        }

        mod mismatch {
            use super::*;

            generate_value_constraint_tests!(
                tests: [
                    {
                        name: bool_setting,
                        settings: bool => &[Some(false)],
                    },
                ],
                constraints: bool => &[
                    ResolvedValueSequenceConstraint {
                        exact: Some(vec![true]),
                        ideal: None,
                    },
                    ResolvedValueSequenceConstraint {
                        exact: Some(vec![true]),
                        ideal: Some(vec![true]),
                    },
                ],
                expected: Err(SettingFitnessDistanceError {
                    kind: SettingFitnessDistanceErrorKind::Mismatch,
                    constraint: "(x == [true])".to_owned(),
                    setting: Some("false".to_owned()),
                })
            );
        }
    }
}
