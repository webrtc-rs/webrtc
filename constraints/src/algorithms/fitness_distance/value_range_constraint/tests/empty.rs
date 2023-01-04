use super::*;

macro_rules! generate_empty_value_range_constraint_tests {
    (
        tests: [
            $({
                name: $ti:ident,
                settings: $st:ty => $se:expr $(,)?
            }),+ $(,)?
        ],
        constraint: $ct:ty $(,)?
    ) => {
        generate_value_range_constraint_tests!(
            tests: [
                $({
                    name: $ti,
                    settings: $st => $se,
                }),+
            ],
            constraints: $ct => &[
                ResolvedValueRangeConstraint::<$ct> {
                    min: None,
                    max: None,
                    exact: None,
                    ideal: None,
                }
            ],
            expected: Ok(0.0)
        );
    };
}

mod u64_constraint {
    use super::*;

    generate_empty_value_range_constraint_tests!(
        tests: [
            {
                name: bool_setting,
                settings: bool => &[Some(false)],
            },
            {
                name: string_setting,
                settings: String => &[Some("foo".to_owned())],
            },
            {
                name: f64_setting,
                settings: f64 => &[Some(42.0)],
            },
        ],
        constraint: u64,
    );
}

mod f64_constraint {
    use super::*;

    generate_empty_value_range_constraint_tests!(
        tests: [
            {
                name: bool_setting,
                settings: bool => &[Some(false)],
            },
            {
                name: string_setting,
                settings: String => &[Some("foo".to_owned())],
            },
            {
                name: i64_setting,
                settings: i64 => &[Some(42)],
            },
        ],
        constraint: f64,
    );
}
