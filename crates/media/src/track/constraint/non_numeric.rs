use crate::track::constraint::Fitness;

// FIXME: use some kind of SmallVec?
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub enum NonNumericMatchesKind<T> {
    Single(T),
    Multiple(Vec<T>),
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub enum NonNumericKind<T> {
    Exists(bool),
    Matches(NonNumericMatchesKind<T>),
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub struct NonNumeric<T> {
    kind: NonNumericKind<T>,
    ideal: Option<T>,
    required: bool,
}

impl<T> NonNumeric<T> {
    pub fn exists(exists: bool) -> Self {
        Self::kind(NonNumericKind::Exists(exists))
    }

    pub fn is(value: T) -> Self
    where
        T: Clone,
    {
        Self::matches(NonNumericMatchesKind::Single(value))
    }

    pub fn any_of(values: Vec<T>) -> Self {
        Self::matches(NonNumericMatchesKind::Multiple(values))
    }

    fn kind(kind: NonNumericKind<T>) -> Self {
        Self {
            kind,
            ideal: None,
            required: false,
        }
    }

    fn matches(kind: NonNumericMatchesKind<T>) -> Self {
        Self::kind(NonNumericKind::Matches(kind))
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

impl<T> Fitness<T> for NonNumeric<T>
where
    T: Clone + PartialOrd,
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
            NonNumericKind::Exists(exists) => {
                if *exists == actual.is_some() {
                    0.0
                } else {
                    mismatch_distance()
                }
            }
            NonNumericKind::Matches(kind) => {
                // TODO(regexident): replace with `let_else` once stabilized:
                // Tracking issue: https://github.com/rust-lang/rust/issues/87335
                let actual = match actual {
                    Some(actual) => actual,
                    None => return mismatch_distance(),
                };

                let matches_value = match kind {
                    NonNumericMatchesKind::Single(value) => value == actual,
                    NonNumericMatchesKind::Multiple(values) => values.contains(actual),
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
                        // > 6. If no ideal value is specified […], the fitness distance is `0`.
                        return 0.0;
                    }
                };

                // Corresponding excerpt from W3C spec:
                //
                // > 8. For all string, enum and boolean constraints […],
                // > the fitness distance is the result of the formula
                // >
                // > ```
                // > (actual == ideal) ? 0 : 1
                // > ```
                if actual == ideal {
                    0.0
                } else {
                    1.0
                }
            }
        }
    }
}
