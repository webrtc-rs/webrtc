//! Fitness function
//!
//! # W3C Spec:
//!
//! > We define the fitness distance between a settings dictionary and a constraint set CS as the sum,
//! > for each member (represented by a `constraintName` and `constraintValue` pair) which exists in CS,
//! > of the following values:
//! >
//! > 1. If `constraintName` is not supported by the User Agent, the fitness distance is `0`.
//! >
//! > 2. If the constraint is required (`constraintValue` either contains one or more members named
//! > '`min`', '`max`', or '`exact`', or is itself a bare value and bare values are to be treated as '`exact`'),
//! > and the settings dictionary's `constraintName` member's value does not satisfy
//! > the constraint or doesn't exist, the fitness distance is positive infinity.
//! >
//! > 3. If the constraint does not apply for this type of object, the fitness distance is `0`
//! > (that is, the constraint does not influence the fitness distance).
//! >
//! > 4. If `constraintValue` is a boolean, but the constrainable property is not,
//! > then the fitness distance is based on whether the settings dictionary's
//! > `constraintName` member exists or not, from the formula
//! >
//! > ```plain
//! > (constraintValue == exists) ? 0 : 1
//! > ```
//! >
//! > 5. If the settings dictionary's `constraintName` member does not exist, the fitness distance is `1`.
//! >
//! > 6. If no ideal value is specified (`constraintValue` either contains no member named '`ideal`',
//! > or, if bare values are to be treated as '`ideal`', isn't a bare value), the fitness distance is `0`.
//! >
//! > 7. For all positive numeric constraints (such as `height`, `width`, `frameRate`, `aspectRatio`, `sampleRate` and `sampleSize`),
//! > the fitness distance is the result of the formula
//! >
//! > ```plain
//! > (actual == ideal) ? 0 : |actual - ideal| / max(|actual|, |ideal|)
//! > ```
//! >
//! > 8. For all string, enum and boolean constraints (e.g. `deviceId`, `groupId`, `facingMode`, `resizeMode`, `echoCancellation`),
//! > the fitness distance is the result of the formula
//! >
//! > ```plain
//! > (actual == ideal) ? 0 : 1
//! > ```
//! >
//! > <https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance>

pub(crate) trait Fitness<Value>: Sized {
    fn fitness_distance(&self, value: Option<&Value>) -> f64;
}
