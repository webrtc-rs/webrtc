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
                ValueConstraint {
                    exact: None,
                    ideal: None,
                },
                ValueConstraint {
                    exact: None,
                    ideal: Some("foo".to_owned()),
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
            constraints: String => &[ValueConstraint {
                exact: None,
                ideal: Some("foo".to_owned()),
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
            constraints: String => &[ValueConstraint {
                exact: Some("foo".to_owned()),
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
                    ValueConstraint {
                        exact: Some("foo".to_owned()),
                        ideal: None,
                    },
                    ValueConstraint {
                        exact: Some("foo".to_owned()),
                        ideal: Some("foo".to_owned()),
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
                        name: string_setting,
                        settings: String => &[Some("bar".to_owned())],
                    },
                ],
                constraints: String => &[
                    ValueConstraint {
                        exact: Some("foo".to_owned()),
                        ideal: None,
                    },
                    ValueConstraint {
                        exact: Some("foo".to_owned()),
                        ideal: Some("foo".to_owned()),
                    },
                ],
                expected: Err(SettingFitnessDistanceError::Mismatch)
            );
        }
    }
}
