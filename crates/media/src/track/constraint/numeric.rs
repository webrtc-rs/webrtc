use std::ops::{RangeFrom, RangeInclusive, RangeToInclusive};

use crate::track::constraint::Fitness;

pub(crate) trait NumericSetting {
    fn float_value(&self) -> f64;
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum NumericMatchesKind<T> {
    AtMost(RangeToInclusive<T>),
    AtLeast(RangeFrom<T>),
    Within(RangeInclusive<T>),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum NumericKind<T> {
    Exists(bool),
    Matches(NumericMatchesKind<T>),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Numeric<T> {
    kind: NumericKind<T>,
    ideal: Option<T>,
    required: bool,
}

impl<T> Numeric<T> {
    pub fn exists(exists: bool) -> Self {
        Self::kind(NumericKind::Exists(exists))
    }

    pub fn at(value: T) -> Self
    where
        T: Clone,
    {
        let range = value.clone()..=value;
        Self::matches(NumericMatchesKind::Within(range))
    }

    pub fn at_most(range: RangeToInclusive<T>) -> Self {
        Self::matches(NumericMatchesKind::AtMost(range))
    }

    pub fn at_least(range: RangeFrom<T>) -> Self {
        Self::matches(NumericMatchesKind::AtLeast(range))
    }

    pub fn within(range: RangeInclusive<T>) -> Self {
        Self::matches(NumericMatchesKind::Within(range))
    }

    fn kind(kind: NumericKind<T>) -> Self {
        Self {
            kind,
            ideal: None,
            required: false,
        }
    }

    fn matches(kind: NumericMatchesKind<T>) -> Self {
        Self::kind(NumericKind::Matches(kind))
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn ideal(mut self, ideal: Option<T>) -> Self {
        self.ideal = ideal;
        self
    }

    pub fn is_required(&self) -> bool {
        self.required
    }

    pub fn ideal_value(&self) -> Option<&T> {
        self.ideal.as_ref()
    }
}

impl<T> Fitness<T> for Numeric<T>
where
    T: Clone + PartialOrd + NumericSetting,
{
    fn fitness_distance(&self, actual: Option<&T>) -> f64 {
        let mismatch_distance = || {
            if self.required {
                // Corresponding excerpt from W3C spec:
                //
                // > 2. If the […] settings dictionary’s constraintName member’s value does not
                // > satisfy the constraint […], the fitness distance is positive infinity.
                f64::INFINITY
            } else {
                // Corresponding excerpt from W3C spec:
                //
                // > 5. If the settings dictionary's `constraintName` member does not exist,
                // > the fitness distance is `1`.
                1.0
            }
        };

        match &self.kind {
            NumericKind::Exists(exists) => {
                // Corresponding excerpt from W3C spec:
                //
                // > 4. If constraintValue is a boolean, but the constrainable property is not,
                // > then the fitness distance is based on whether the settings
                // > dictionary's constraintName member exists or not, from the formula
                // >
                // > ```
                // > (constraintValue == exists) ? 0 : 1
                // > ```
                if *exists == actual.is_some() {
                    0.0
                } else {
                    mismatch_distance()
                }
            }
            NumericKind::Matches(kind) => {
                // TODO(regexident): replace with `let_else` once stabilized:
                // Tracking issue: https://github.com/rust-lang/rust/issues/87335
                let actual = match actual {
                    Some(actual) => actual,
                    None => return mismatch_distance(),
                };

                let matches_value = match kind {
                    NumericMatchesKind::AtMost(range) => range.contains(actual),
                    NumericMatchesKind::AtLeast(range) => range.contains(actual),
                    NumericMatchesKind::Within(range) => range.contains(actual),
                };

                if !matches_value {
                    return mismatch_distance();
                }

                // TODO(regexident): replace with `let_else` once stabilized:
                // Tracking issue: https://github.com/rust-lang/rust/issues/87335
                let ideal = match self.ideal.as_ref() {
                    Some(ideal) => ideal,
                    None => {
                        // Corresponding excerpt from W3C spec:
                        //
                        // > 6. If no ideal value is specified, the fitness distance is `0`.
                        return 0.0;
                    }
                };

                // Corresponding excerpt from W3C spec:
                //
                // > 7. For all positive numeric constraints […],
                // > the fitness distance is the result of the formula
                // >
                // > ```
                // > (actual == ideal) ? 0 : |actual - ideal| / max(|actual|, |ideal|)
                // > ```
                if actual == ideal {
                    return 0.0;
                } else {
                    let actual: f64 = actual.float_value();
                    let ideal: f64 = ideal.float_value();

                    let numerator = (actual - ideal).abs();
                    let denominator = actual.abs().max(ideal.abs());
                    numerator / denominator
                }
            }
        }
    }
}
