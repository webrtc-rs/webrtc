use crate::track::constraint::Fitness;

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum NonNumericKind<T> {
    Exists { is_expected: bool },
    Exactly { value: T },
    AnyOf { values: Vec<T>, ideal: Option<T> },
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct NonNumeric<T> {
    is_required: bool,
    kind: NonNumericKind<T>,
}

impl<T> NonNumeric<T>
where
    T: Clone + PartialEq,
{
    pub fn new(is_required: bool, kind: NonNumericKind<T>) -> Self {
        Self { is_required, kind }
    }

    pub fn exists(is_expected: bool) -> Self {
        Self::new(false, NonNumericKind::Exists { is_expected })
    }

    pub fn exactly(value: T) -> Self {
        Self::new(false, NonNumericKind::Exactly { value })
    }

    pub fn any_of(values: Vec<T>, ideal: Option<T>) -> Self {
        if let Some(ideal) = &ideal {
            assert!(values.contains(ideal));
        }

        Self::new(false, NonNumericKind::AnyOf { values, ideal })
    }

    pub fn is_required(mut self, is_required: bool) -> Self {
        self.is_required = is_required;
        self
    }
}

impl<T> Fitness<T> for NonNumeric<T>
where
    T: Clone + PartialEq,
{
    fn fitness_distance(&self, actual: Option<&T>) -> f64 {
        let (is_match, ideal) = match &self.kind {
            NonNumericKind::Exists { is_expected } => {
                let is_match = actual.is_some() == *is_expected;
                (is_match, None)
            }
            NonNumericKind::Exactly { value } => {
                let is_match = match actual {
                    Some(actual) => value == actual,
                    None => false,
                };
                (is_match, Some(value))
            }
            NonNumericKind::AnyOf { values, ideal } => {
                let is_match = match actual {
                    Some(actual) => values.contains(actual),
                    None => false,
                };
                (is_match, ideal.as_ref())
            }
        };

        if !is_match {
            if self.is_required {
                return f64::INFINITY;
            } else {
                return 1.0;
            }
        }

        match (actual, ideal) {
            (_, None) => {
                // No ideal value specified, so all values are equally fit.
                0.0
            }
            (None, Some(_)) => {
                // Value missing, so all values are equally unfit.
                1.0
            }
            (Some(actual), Some(ideal)) => {
                if actual == ideal {
                    0.0
                } else {
                    1.0
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
    pub struct Dummy(&'static str);

    impl From<&'static str> for Dummy {
        fn from(u: &'static str) -> Self {
            Self(u)
        }
    }

    #[test]
    fn exists() {
        let subject = NonNumeric::<Dummy>::exists(true);
        assert_eq!(
            subject,
            NonNumeric {
                is_required: false,
                kind: NonNumericKind::Exists { is_expected: true }
            }
        );
    }

    #[test]
    fn exactly() {
        let subject = NonNumeric::<Dummy>::exactly(Dummy("x"));
        assert_eq!(
            subject,
            NonNumeric {
                is_required: false,
                kind: NonNumericKind::Exactly { value: Dummy("x") }
            }
        );
    }

    #[test]
    fn any_of() {
        let subject: NonNumeric<Dummy> =
            NonNumeric::any_of(vec![Dummy("x"), Dummy("o")], Some(Dummy("x")));
        assert_eq!(
            subject,
            NonNumeric {
                is_required: false,
                kind: NonNumericKind::AnyOf {
                    values: vec![Dummy("x"), Dummy("o")],
                    ideal: Some(Dummy("x"))
                }
            }
        );
    }

    #[test]
    fn is_required() {
        let subject = NonNumeric::<Dummy>::exists(true);
        assert_eq!(subject.is_required, false);
        let subject = subject.is_required(true);
        assert_eq!(subject.is_required, true);
        let subject = subject.is_required(false);
        assert_eq!(subject.is_required, false);
    }

    #[test]
    fn fitness_distance_exists() {
        fn fitness(is_expected: bool, is_required: bool, setting: Option<&'static str>) -> f64 {
            let actual = setting.map(|t| t.into());
            NonNumeric::<Dummy>::exists(is_expected)
                .is_required(is_required)
                .fitness_distance(actual.as_ref())
        }

        assert_eq!(fitness(false, false, None), 0.0);
        assert_eq!(fitness(false, false, Some("x")), 1.0);
        assert_eq!(fitness(false, true, None), 0.0);
        assert_eq!(fitness(false, true, Some("x")), f64::INFINITY);
        assert_eq!(fitness(true, false, None), 1.0);
        assert_eq!(fitness(true, false, Some("x")), 0.0);
        assert_eq!(fitness(true, true, None), f64::INFINITY);
        assert_eq!(fitness(true, true, Some("x")), 0.0);
    }

    #[test]
    fn fitness_distance_exactly() {
        fn fitness(value: &'static str, is_required: bool, setting: Option<&'static str>) -> f64 {
            let actual = setting.map(|t| t.into());
            NonNumeric::<Dummy>::exactly(value.into())
                .is_required(is_required)
                .fitness_distance(actual.as_ref())
        }

        assert_eq!(fitness("x", false, None), 1.0);
        assert_eq!(fitness("x", false, Some("x")), 0.0);
        assert_eq!(fitness("x", true, None), f64::INFINITY);
        assert_eq!(fitness("x", true, Some("x")), 0.0);
        assert_eq!(fitness("0", false, None), 1.0);
        assert_eq!(fitness("0", false, Some("x")), 1.0);
        assert_eq!(fitness("0", true, None), f64::INFINITY);
        assert_eq!(fitness("0", true, Some("x")), f64::INFINITY);
    }

    #[test]
    fn fitness_distance_any_of() {
        fn fitness(
            values: Vec<&'static str>,
            ideal: Option<&'static str>,
            is_required: bool,
            setting: Option<&'static str>,
        ) -> f64 {
            let values = values.into_iter().map(|t| t.into()).collect();
            let ideal = ideal.map(|t| t.into());
            let actual = setting.map(|t| t.into());
            NonNumeric::<Dummy>::any_of(values, ideal)
                .is_required(is_required)
                .fitness_distance(actual.as_ref())
        }

        assert_eq!(fitness(vec!["x"], None, false, None), 1.0);
        assert_eq!(fitness(vec!["x"], None, false, Some("x")), 0.0);
        assert_eq!(fitness(vec!["x"], None, true, None), f64::INFINITY);
        assert_eq!(fitness(vec!["x"], None, true, Some("x")), 0.0);
        assert_eq!(fitness(vec!["x"], Some("x"), false, None), 1.0);
        assert_eq!(fitness(vec!["x"], Some("x"), false, Some("x")), 0.0);
        assert_eq!(fitness(vec!["x"], Some("x"), true, None), f64::INFINITY);
        assert_eq!(fitness(vec!["x"], Some("x"), true, Some("x")), 0.0);
        assert_eq!(fitness(vec!["o"], None, false, None), 1.0);
        assert_eq!(fitness(vec!["o"], None, false, Some("x")), 1.0);
        assert_eq!(fitness(vec!["o"], None, true, None), f64::INFINITY);
        assert_eq!(fitness(vec!["o"], None, true, Some("x")), f64::INFINITY);
        assert_eq!(fitness(vec!["o"], Some("o"), false, None), 1.0);
        assert_eq!(fitness(vec!["o"], Some("o"), false, Some("x")), 1.0);
        assert_eq!(fitness(vec!["o"], Some("o"), true, None), f64::INFINITY);
        assert_eq!(
            fitness(vec!["o"], Some("o"), true, Some("x")),
            f64::INFINITY
        );
    }
}
