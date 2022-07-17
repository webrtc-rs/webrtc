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
                ValueConstraint {
                    exact: None,
                    ideal: None,
                },
                ValueConstraint {
                    exact: None,
                    ideal: Some(true),
                },
            ],
            expected: Ok(0.0)
        );

        generate_value_constraint_tests!(
            tests: [
                {
                    name: string_setting,
                    settings: String => &[Some("foo".to_owned())],
                },
                {
                    name: i64_setting,
                    settings: i64 => &[Some(42)],
                },
                {
                    name: f64_setting,
                    settings: f64 => &[Some(42.0)],
                },
            ],
            constraints: bool => &[
                ValueConstraint {
                    exact: None,
                    ideal: Some(false),
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
            constraints: bool => &[ValueConstraint {
                exact: None,
                ideal: Some(true),
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
            constraints: bool => &[ValueConstraint {
                exact: Some(true),
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
                    ValueConstraint {
                        exact: Some(true),
                        ideal: None,
                    },
                    ValueConstraint {
                        exact: Some(true),
                        ideal: Some(true),
                    },
                ],
                expected: Err(SettingFitnessDistanceError::Missing)
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
                    ValueConstraint {
                        exact: Some(true),
                        ideal: None,
                    },
                    ValueConstraint {
                        exact: Some(true),
                        ideal: Some(true),
                    },
                ],
                expected: Err(SettingFitnessDistanceError::Mismatch)
            );
        }
    }

    // Required boolean constraints have specialized logic as per
    // rule 4 of the fitness distance algorithm specification:
    // https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance

    mod specialization {
        use super::*;

        mod expected {
            use super::*;

            mod existing {
                use super::*;

                generate_value_constraint_tests!(
                    tests: [
                        {
                            name: string_setting,
                            settings: String => &[Some("foo".to_owned())],
                        },
                        {
                            name: i64_setting,
                            settings: i64 => &[Some(42)],
                        },
                        {
                            name: f64_setting,
                            settings: f64 => &[Some(42.0)],
                        },
                    ],
                    constraints: bool => &[
                        ValueConstraint {
                            exact: Some(true),
                            ideal: None,
                        },
                    ],
                    expected: Ok(0.0)
                );
            }

            mod missing {
                use super::*;

                generate_value_constraint_tests!(
                    tests: [
                        {
                            name: string_setting,
                            settings: String => &[None],
                        },
                        {
                            name: i64_setting,
                            settings: i64 => &[None],
                        },
                        {
                            name: f64_setting,
                            settings: f64 => &[None],
                        },
                    ],
                    constraints: bool => &[
                        ValueConstraint {
                            exact: Some(true),
                            ideal: None,
                        },
                    ],
                    expected: Ok(1.0)
                );
            }
        }

        mod unexpected {
            use super::*;

            mod existing {
                use super::*;

                generate_value_constraint_tests!(
                    tests: [
                        {
                            name: string_setting,
                            settings: String => &[Some("foo".to_owned())],
                        },
                        {
                            name: i64_setting,
                            settings: i64 => &[Some(42)],
                        },
                        {
                            name: f64_setting,
                            settings: f64 => &[Some(42.0)],
                        },
                    ],
                    constraints: bool => &[
                        ValueConstraint {
                            exact: Some(false),
                            ideal: None,
                        },
                    ],
                    expected: Ok(1.0)
                );
            }

            mod missing {
                use super::*;

                generate_value_constraint_tests!(
                    tests: [
                        {
                            name: string_setting,
                            settings: String => &[None],
                        },
                        {
                            name: i64_setting,
                            settings: i64 => &[None],
                        },
                        {
                            name: f64_setting,
                            settings: f64 => &[None],
                        },
                    ],
                    constraints: bool => &[
                        ValueConstraint {
                            exact: Some(false),
                            ideal: None,
                        },
                    ],
                    expected: Ok(0.0)
                );
            }
        }
    }
}
