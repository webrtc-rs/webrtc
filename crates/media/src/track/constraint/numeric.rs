use crate::track::{constraint::Fitness, setting::NumericSetting};

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum NumericKind<T> {
    Exists { is_expected: bool },
    Exactly { value: T },
    AtLeast { min: T, ideal: Option<T> },
    AtMost { max: T, ideal: Option<T> },
    Within { min: T, max: T, ideal: Option<T> },
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Numeric<T> {
    is_required: bool,
    kind: NumericKind<T>,
}

impl<T> Numeric<T>
where
    T: Clone + PartialOrd,
{
    pub fn new(is_required: bool, kind: NumericKind<T>) -> Self {
        Self { is_required, kind }
    }

    pub fn exists(is_expected: bool) -> Self {
        Self::new(false, NumericKind::Exists { is_expected })
    }

    pub fn exactly(value: T) -> Self {
        Self::new(false, NumericKind::Exactly { value })
    }

    pub fn at_least(min: T, ideal: Option<T>) -> Self {
        if let Some(ideal) = &ideal {
            assert!(min <= *ideal);
        }
        Self::new(false, NumericKind::AtLeast { min, ideal })
    }

    pub fn at_most(max: T, ideal: Option<T>) -> Self {
        if let Some(ideal) = &ideal {
            assert!(max >= *ideal);
        }
        Self::new(false, NumericKind::AtMost { max, ideal })
    }

    pub fn within(min: T, max: T, ideal: Option<T>) -> Self {
        if let Some(ideal) = &ideal {
            assert!(min <= *ideal);
            assert!(max >= *ideal);
        }
        Self::new(false, NumericKind::Within { min, max, ideal })
    }

    pub fn is_required(mut self, is_required: bool) -> Self {
        self.is_required = is_required;
        self
    }
}

impl<T> Fitness<T> for Numeric<T>
where
    T: Clone + PartialOrd + NumericSetting,
{
    fn fitness_distance(&self, actual: Option<&T>) -> f64 {
        let (is_match, ideal) = match &self.kind {
            NumericKind::Exists { is_expected } => {
                let is_match = actual.is_some() == *is_expected;
                (is_match, None)
            }
            NumericKind::Exactly { value } => {
                let is_match = match actual {
                    Some(actual) => value == actual,
                    None => false,
                };
                (is_match, Some(value))
            }
            NumericKind::AtLeast { min, ideal } => {
                let is_match = match actual {
                    Some(actual) => (min..).contains(&actual),
                    None => false,
                };
                (is_match, ideal.as_ref())
            }
            NumericKind::AtMost { max, ideal } => {
                let is_match = match actual {
                    Some(actual) => (..=max).contains(&actual),
                    None => false,
                };
                (is_match, ideal.as_ref())
            }
            NumericKind::Within { min, max, ideal } => {
                let is_match = match actual {
                    Some(actual) => (min..=max).contains(&actual),
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
                let actual: f64 = actual.float_value();
                let ideal: f64 = ideal.float_value();

                let numerator = (actual - ideal).abs();
                let denominator = actual.abs().max(ideal.abs());
                numerator / denominator
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
    pub struct Dummy(u32);

    impl From<u32> for Dummy {
        fn from(u: u32) -> Self {
            Self(u)
        }
    }

    impl NumericSetting for Dummy {
        fn float_value(&self) -> f64 {
            self.0 as f64
        }
    }

    #[test]
    fn exists() {
        let subject = Numeric::<Dummy>::exists(true);
        assert_eq!(
            subject,
            Numeric {
                is_required: false,
                kind: NumericKind::Exists { is_expected: true }
            }
        );
    }

    #[test]
    fn exactly() {
        let subject = Numeric::<Dummy>::exactly(Dummy(42));
        assert_eq!(
            subject,
            Numeric {
                is_required: false,
                kind: NumericKind::Exactly { value: Dummy(42) }
            }
        );
    }

    #[test]
    fn at_least() {
        let subject = Numeric::<Dummy>::at_least(Dummy(10), Some(Dummy(42)));
        assert_eq!(
            subject,
            Numeric {
                is_required: false,
                kind: NumericKind::AtLeast {
                    min: Dummy(10),
                    ideal: Some(Dummy(42))
                }
            }
        );
    }

    #[test]
    fn at_most() {
        let subject = Numeric::<Dummy>::at_most(Dummy(100), Some(Dummy(42)));
        assert_eq!(
            subject,
            Numeric {
                is_required: false,
                kind: NumericKind::AtMost {
                    max: Dummy(100),
                    ideal: Some(Dummy(42))
                }
            }
        );
    }

    #[test]
    fn within() {
        let subject = Numeric::<Dummy>::within(Dummy(10), Dummy(100), Some(Dummy(42)));
        assert_eq!(
            subject,
            Numeric {
                is_required: false,
                kind: NumericKind::Within {
                    min: Dummy(10),
                    max: Dummy(100),
                    ideal: Some(Dummy(42))
                }
            }
        );
    }

    #[test]
    fn is_required() {
        let subject = Numeric::<Dummy>::exists(true);
        assert_eq!(subject.is_required, false);
        let subject = subject.is_required(true);
        assert_eq!(subject.is_required, true);
        let subject = subject.is_required(false);
        assert_eq!(subject.is_required, false);
    }

    #[test]
    fn fitness_distance_exists() {
        fn fitness(is_expected: bool, is_required: bool, setting: Option<u32>) -> f64 {
            let actual = setting.map(|t| t.into());
            Numeric::<Dummy>::exists(is_expected)
                .is_required(is_required)
                .fitness_distance(actual.as_ref())
        }

        assert_eq!(fitness(false, false, None), 0.0);
        assert_eq!(fitness(false, false, Some(42)), 1.0);
        assert_eq!(fitness(false, true, None), 0.0);
        assert_eq!(fitness(false, true, Some(42)), f64::INFINITY);
        assert_eq!(fitness(true, false, None), 1.0);
        assert_eq!(fitness(true, false, Some(42)), 0.0);
        assert_eq!(fitness(true, true, None), f64::INFINITY);
        assert_eq!(fitness(true, true, Some(42)), 0.0);
    }

    #[test]
    fn fitness_distance_exactly() {
        fn fitness(value: u32, is_required: bool, setting: Option<u32>) -> f64 {
            let actual = setting.map(|t| t.into());
            Numeric::<Dummy>::exactly(value.into())
                .is_required(is_required)
                .fitness_distance(actual.as_ref())
        }

        assert_eq!(fitness(42, false, None), 1.0);
        assert_eq!(fitness(42, false, Some(42)), 0.0);
        assert_eq!(fitness(42, true, None), f64::INFINITY);
        assert_eq!(fitness(42, true, Some(42)), 0.0);
        assert_eq!(fitness(123, false, None), 1.0);
        assert_eq!(fitness(123, false, Some(42)), 1.0);
        assert_eq!(fitness(123, true, None), f64::INFINITY);
        assert_eq!(fitness(123, true, Some(42)), f64::INFINITY);
    }

    #[test]
    fn fitness_distance_at_least() {
        fn fitness(min: u32, ideal: Option<u32>, is_required: bool, setting: Option<u32>) -> f64 {
            let ideal = ideal.map(|t| t.into());
            let actual = setting.map(|t| t.into());
            Numeric::<Dummy>::at_least(min.into(), ideal)
                .is_required(is_required)
                .fitness_distance(actual.as_ref())
        }

        assert_eq!(fitness(10, None, false, None), 1.0);
        assert_eq!(fitness(10, None, false, Some(42)), 0.0);
        assert_eq!(fitness(10, None, true, None), f64::INFINITY);
        assert_eq!(fitness(10, None, true, Some(42)), 0.0);
        assert_eq!(fitness(10, Some(50), false, None), 1.0);
        assert_eq!(fitness(10, Some(50), false, Some(42)), 0.16);
        assert_eq!(fitness(10, Some(50), true, None), f64::INFINITY);
        assert_eq!(fitness(10, Some(50), true, Some(42)), 0.16);
        assert_eq!(fitness(100, None, false, None), 1.0);
        assert_eq!(fitness(100, None, false, Some(42)), 1.0);
        assert_eq!(fitness(100, None, true, None), f64::INFINITY);
        assert_eq!(fitness(100, None, true, Some(42)), f64::INFINITY);
        assert_eq!(fitness(100, Some(200), false, None), 1.0);
        assert_eq!(fitness(100, Some(200), false, Some(42)), 1.0);
        assert_eq!(fitness(100, Some(200), true, None), f64::INFINITY);
        assert_eq!(fitness(100, Some(200), true, Some(42)), f64::INFINITY);
    }

    #[test]
    fn fitness_distance_at_most() {
        fn fitness(max: u32, ideal: Option<u32>, is_required: bool, setting: Option<u32>) -> f64 {
            let ideal = ideal.map(|t| t.into());
            let actual = setting.map(|t| t.into());
            Numeric::<Dummy>::at_most(max.into(), ideal)
                .is_required(is_required)
                .fitness_distance(actual.as_ref())
        }

        assert_eq!(fitness(100, None, false, None), 1.0);
        assert_eq!(fitness(100, None, false, Some(42)), 0.0);
        assert_eq!(fitness(100, None, true, None), f64::INFINITY);
        assert_eq!(fitness(100, None, true, Some(42)), 0.0);
        assert_eq!(fitness(100, Some(50), false, None), 1.0);
        assert_eq!(fitness(100, Some(50), false, Some(42)), 0.16);
        assert_eq!(fitness(100, Some(50), true, None), f64::INFINITY);
        assert_eq!(fitness(100, Some(50), true, Some(42)), 0.16);
        assert_eq!(fitness(10, None, false, None), 1.0);
        assert_eq!(fitness(10, None, false, Some(42)), 1.0);
        assert_eq!(fitness(10, None, true, None), f64::INFINITY);
        assert_eq!(fitness(10, None, true, Some(42)), f64::INFINITY);
        assert_eq!(fitness(10, Some(5), false, None), 1.0);
        assert_eq!(fitness(10, Some(5), false, Some(42)), 1.0);
        assert_eq!(fitness(10, Some(5), true, None), f64::INFINITY);
        assert_eq!(fitness(10, Some(5), true, Some(42)), f64::INFINITY);
    }

    #[test]
    fn fitness_distance_within() {
        fn fitness(
            min: u32,
            max: u32,
            ideal: Option<u32>,
            is_required: bool,
            setting: Option<u32>,
        ) -> f64 {
            let ideal = ideal.map(|t| t.into());
            let actual = setting.map(|t| t.into());
            Numeric::<Dummy>::within(min.into(), max.into(), ideal)
                .is_required(is_required)
                .fitness_distance(actual.as_ref())
        }

        assert_eq!(fitness(10, 100, None, false, None), 1.0);
        assert_eq!(fitness(10, 100, None, false, Some(42)), 0.0);
        assert_eq!(fitness(10, 100, None, true, None), f64::INFINITY);
        assert_eq!(fitness(10, 100, None, true, Some(42)), 0.0);
        assert_eq!(fitness(10, 100, Some(50), false, None), 1.0);
        assert_eq!(fitness(10, 100, Some(50), false, Some(42)), 0.16);
        assert_eq!(fitness(10, 100, Some(50), true, None), f64::INFINITY);
        assert_eq!(fitness(10, 100, Some(50), true, Some(42)), 0.16);
        assert_eq!(fitness(10, 20, None, false, None), 1.0);
        assert_eq!(fitness(10, 20, None, false, Some(42)), 1.0);
        assert_eq!(fitness(10, 20, None, true, None), f64::INFINITY);
        assert_eq!(fitness(10, 20, None, true, Some(42)), f64::INFINITY);
        assert_eq!(fitness(10, 20, Some(15), false, None), 1.0);
        assert_eq!(fitness(10, 20, Some(15), false, Some(42)), 1.0);
        assert_eq!(fitness(10, 20, Some(15), true, None), f64::INFINITY);
        assert_eq!(fitness(10, 20, Some(15), true, Some(42)), f64::INFINITY);
    }
}
