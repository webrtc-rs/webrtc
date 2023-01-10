use crate::algorithms::SettingFitnessDistanceErrorKind;

use super::*;

mod basic {
    use super::*;

    mod zero_distance {
        use super::*;

        generate_value_range_constraint_tests!(
            tests: [
                {
                    name: i64_setting,
                    settings: i64 => &[Some(42)],
                },
                {
                    name: f64_setting,
                    settings: f64 => &[Some(42.0)],
                },
            ],
            constraints: u64 => &[ResolvedValueRangeConstraint {
                min: None,
                max: None,
                exact: None,
                ideal: Some(42),
            }],
            expected: Ok(0.0)
        );

        generate_value_range_constraint_tests!(
            tests: [
                {
                    name: bool_setting,
                    settings: bool => &[Some(true)],
                },
                {
                    name: string_setting,
                    settings: String => &[Some("foo".to_owned())],
                },
            ],
            constraints: u64 => &[
                ResolvedValueRangeConstraint {
                    min: None,
                    max: None,
                    exact: None,
                    ideal: Some(42),
                }
            ],
            expected: Ok(0.0)
        );
    }

    mod fract_distance {
        use super::*;

        #[test]
        fn i64_setting() {
            test_value_range_constraint!(
                checks: [
                    {
                        setting: i64 => Some(1),
                        constraint: u64 => ResolvedValueRangeConstraint {
                            min: None,
                            max: None,
                            exact: None,
                            ideal: Some(4),
                        },
                        expected: Ok(0.75),
                    },
                    {
                        setting: i64 => Some(2),
                        constraint: u64 => ResolvedValueRangeConstraint {
                            min: None,
                            max: None,
                            exact: None,
                            ideal: Some(4),
                        },
                        expected: Ok(0.5),
                    },
                    {
                        setting: i64 => Some(3),
                        constraint: u64 => ResolvedValueRangeConstraint {
                            min: None,
                            max: None,
                            exact: None,
                            ideal: Some(4),
                        },
                        expected: Ok(0.25),
                    },
                ],
                validate: |actual, expected| {
                    assert_eq!(actual, expected);
                }
            );
        }

        #[test]
        fn f64_setting() {
            test_value_range_constraint!(
                checks: [
                    {
                        setting: f64 => Some(1.0),
                        constraint: u64 => ResolvedValueRangeConstraint {
                            min: None,
                            max: None,
                            exact: None,
                            ideal: Some(4),
                        },
                        expected: Ok(0.75),
                    },
                    {
                        setting: f64 => Some(2.0),
                        constraint: u64 => ResolvedValueRangeConstraint {
                            min: None,
                            max: None,
                            exact: None,
                            ideal: Some(4),
                        },
                        expected: Ok(0.5),
                    },
                    {
                        setting: f64 => Some(3.0),
                        constraint: u64 => ResolvedValueRangeConstraint {
                            min: None,
                            max: None,
                            exact: None,
                            ideal: Some(4),
                        },
                        expected: Ok(0.25),
                    },
                ],
                validate: |actual, expected| {
                    assert_eq!(actual, expected);
                }
            );
        }
    }

    mod one_distance {
        use super::*;

        generate_value_range_constraint_tests!(
            tests: [
                {
                    name: i64_setting,
                    settings: i64 => &[None, Some(0)],
                },
                {
                    name: f64_setting,
                    settings: f64 => &[None, Some(0.0)],
                },
            ],
            constraints: u64 => &[ResolvedValueRangeConstraint {
                min: None,
                max: None,
                exact: None,
                ideal: Some(42),
            }],
            expected: Ok(1.0)
        );
    }
}

mod required {
    use super::*;

    mod zero_distance {
        use super::*;

        generate_value_range_constraint_tests!(
            tests: [
                {
                    name: i64_setting,
                    settings: i64 => &[Some(42)],
                },
                {
                    name: f64_setting,
                    settings: f64 => &[Some(42.0)],
                },
            ],
            constraints: u64 => &[ResolvedValueRangeConstraint {
                min: None,
                max: None,
                exact: Some(42),
                ideal: None,
            }],
            expected: Ok(0.0)
        );

        generate_value_range_constraint_tests!(
            tests: [
                {
                    name: bool_setting,
                    settings: bool => &[Some(true)],
                },
                {
                    name: string_setting,
                    settings: String => &[Some("foo".to_owned())],
                },
            ],
            constraints: u64 => &[
                ResolvedValueRangeConstraint {
                    min: None,
                    max: None,
                    exact: Some(42),
                    ideal: None,
                }
            ],
            expected: Ok(0.0)
        );
    }

    mod inf_distance {
        use super::*;

        mod missing {
            use super::*;

            generate_value_range_constraint_tests!(
                tests: [
                    {
                        name: i64_setting,
                        settings: i64 => &[None],
                    },
                    {
                        name: f64_setting,
                        settings: f64 => &[None],
                    },
                ],
                constraints: u64 => &[
                    ResolvedValueRangeConstraint {
                        min: None,
                        max: None,
                        exact: Some(42),
                        ideal: None,
                    },
                    ResolvedValueRangeConstraint {
                        min: None,
                        max: None,
                        exact: Some(42),
                        ideal: Some(42),
                    },
                ],
                expected: Err(SettingFitnessDistanceError {
                    kind: SettingFitnessDistanceErrorKind::Missing,
                    constraint: "(x == 42)".to_owned(),
                    setting: None,
                })
            );
        }

        mod mismatch {
            use super::*;

            generate_value_range_constraint_tests!(
                tests: [
                    {
                        name: i64_setting,
                        settings: i64 => &[Some(0)],
                    },
                ],
                constraints: u64 => &[
                    ResolvedValueRangeConstraint {
                        min: None,
                        max: None,
                        exact: Some(42),
                        ideal: None,
                    },
                    ResolvedValueRangeConstraint {
                        min: None,
                        max: None,
                        exact: Some(42),
                        ideal: Some(42),
                    },
                ],
                expected: Err(SettingFitnessDistanceError {
                    kind: SettingFitnessDistanceErrorKind::Mismatch,
                    constraint: "(x == 42)".to_owned(),
                    setting: Some("0".to_owned()),
                })
            );

            generate_value_range_constraint_tests!(
                tests: [
                    {
                        name: f64_setting,
                        settings: f64 => &[Some(0.0)],
                    },
                ],
                constraints: u64 => &[
                    ResolvedValueRangeConstraint {
                        min: None,
                        max: None,
                        exact: Some(42),
                        ideal: None,
                    },
                    ResolvedValueRangeConstraint {
                        min: None,
                        max: None,
                        exact: Some(42),
                        ideal: Some(42),
                    },
                ],
                expected: Err(SettingFitnessDistanceError {
                    kind: SettingFitnessDistanceErrorKind::Mismatch,
                    constraint: "(x == 42)".to_owned(),
                    setting: Some("0.0".to_owned()),
                })
            );
        }
    }
}
