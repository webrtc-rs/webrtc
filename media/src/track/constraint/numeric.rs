use crate::track::constraint::Fitness;

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum NumericKind<T> {
    Exists { is_expected: bool },
    Exactly { value: T },
    AtLeast { min: T, ideal: Option<T> },
    AtMost { max: T, ideal: Option<T> },
    Within { min: T, max: T, ideal: Option<T> },
}

pub(crate) trait NumericSetting<T> {
    fn float_value(setting: &T) -> f64;
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Numeric<T> {
    is_required: bool,
    kind: NumericKind<T>,
}

impl NumericSetting<u32> for Numeric<u32> {
    fn float_value(setting: &u32) -> f64 {
        (*setting) as f64
    }
}

impl NumericSetting<f64> for Numeric<f64> {
    fn float_value(setting: &f64) -> f64 {
        *setting
    }
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
    T: Clone + PartialOrd,
    Self: NumericSetting<T>,
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
                let actual: f64 = Self::float_value(actual);
                let ideal: f64 = Self::float_value(ideal);

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

    #[test]
    fn exists() {
        let subject = Numeric::<u32>::exists(true);
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
        let subject = Numeric::<u32>::exactly(42);
        assert_eq!(
            subject,
            Numeric {
                is_required: false,
                kind: NumericKind::Exactly { value: 42 }
            }
        );
    }

    #[test]
    fn at_least() {
        let subject = Numeric::<u32>::at_least(10, Some(42));
        assert_eq!(
            subject,
            Numeric {
                is_required: false,
                kind: NumericKind::AtLeast {
                    min: 10,
                    ideal: Some(42)
                }
            }
        );
    }

    #[test]
    fn at_most() {
        let subject = Numeric::<u32>::at_most(100, Some(42));
        assert_eq!(
            subject,
            Numeric {
                is_required: false,
                kind: NumericKind::AtMost {
                    max: 100,
                    ideal: Some(42)
                }
            }
        );
    }

    #[test]
    fn within() {
        let subject = Numeric::<u32>::within(10, 100, Some(42));
        assert_eq!(
            subject,
            Numeric {
                is_required: false,
                kind: NumericKind::Within {
                    min: 10,
                    max: 100,
                    ideal: Some(42)
                }
            }
        );
    }

    #[test]
    fn is_required() {
        let subject = Numeric::<u32>::exists(true);
        assert_eq!(subject.is_required, false);
        let subject = subject.is_required(true);
        assert_eq!(subject.is_required, true);
        let subject = subject.is_required(false);
        assert_eq!(subject.is_required, false);
    }

    #[test]
    fn fitness_distance_exists() {
        fn fitness(is_expected: bool, is_required: bool, setting: Option<u32>) -> f64 {
            let actual = setting;
            Numeric::<u32>::exists(is_expected)
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
            let actual = setting;
            Numeric::<u32>::exactly(value)
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
            let ideal = ideal;
            let actual = setting;
            Numeric::<u32>::at_least(min, ideal)
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
            let ideal = ideal;
            let actual = setting;
            Numeric::<u32>::at_most(max, ideal)
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
            let ideal = ideal;
            let actual = setting;
            Numeric::<u32>::within(min, max, ideal)
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
