use crate::algorithms::SettingFitnessDistanceErrorKind;

use super::*;

mod basic {
    use super::*;

    mod zero_distance {
        use super::*;

        generate_value_constraint_tests!(
            tests: [
                {
                    name: string_setting,
                    settings: String => &[Some("foo".to_owned())],
                },
            ],
            constraints: String => &[
                ResolvedValueSequenceConstraint {
                    exact: None,
                    ideal: None,
                },
                ResolvedValueSequenceConstraint {
                    exact: None,
                    ideal: Some(vec!["foo".to_owned()]),
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
                    name: string_setting,
                    settings: String => &[None, Some("bar".to_owned())],
                },
            ],
            constraints: String => &[ResolvedValueSequenceConstraint {
                exact: None,
                ideal: Some(vec!["foo".to_owned()]),
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
                    name: string_setting,
                    settings: String => &[Some("foo".to_owned())],
                },
            ],
            constraints: String => &[ResolvedValueSequenceConstraint {
                exact: Some(vec!["foo".to_owned()]),
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
                        name: string_setting,
                        settings: String => &[None],
                    },
                ],
                constraints: String => &[
                    ResolvedValueSequenceConstraint {
                        exact: Some(vec!["foo".to_owned(), "bar".to_owned(), "baz".to_owned()]),
                        ideal: None,
                    },
                    ResolvedValueSequenceConstraint {
                        exact: Some(vec!["foo".to_owned(), "bar".to_owned(), "baz".to_owned()]),
                        ideal: Some(vec!["foo".to_owned()]),
                    },
                ],
                expected: Err(SettingFitnessDistanceError {
                    kind: SettingFitnessDistanceErrorKind::Missing,
                    constraint: "(x == [\"foo\", \"bar\", \"baz\"])".to_owned(),
                    setting: None,
                })
            );
        }

        mod mismatch {
            use super::*;

            generate_value_constraint_tests!(
                tests: [
                    {
                        name: string_setting,
                        settings: String => &[Some("blee".to_owned())],
                    },
                ],
                constraints: String => &[
                    ResolvedValueSequenceConstraint {
                        exact: Some(vec!["foo".to_owned(), "bar".to_owned(), "baz".to_owned()]),
                        ideal: None,
                    },
                    ResolvedValueSequenceConstraint {
                        exact: Some(vec!["foo".to_owned(), "bar".to_owned(), "baz".to_owned()]),
                        ideal: Some(vec!["foo".to_owned()]),
                    },
                ],
                expected: Err(SettingFitnessDistanceError {
                    kind: SettingFitnessDistanceErrorKind::Mismatch,
                    constraint: "(x == [\"foo\", \"bar\", \"baz\"])".to_owned(),
                    setting: Some("\"blee\"".to_owned()),
                })
            );
        }
    }
}
