/// Serializes a `tokio::time::Instant` to an approximation of epoch time in the form
/// of an `f64` where the integer portion is seconds and the decimal portion is milliseconds.
/// For instance, `Monday, May 30, 2022 10:45:26.456 PM UTC` converts to `1653950726.456`.
///
/// Note that an `Instant` is not connected to real world time, so this conversion is
/// approximate.
pub mod instant_to_epoch_seconds {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tokio::time::Instant;

    pub fn serialize<S>(instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let system_now = SystemTime::now();
        let instant_now = Instant::now();
        let approx = system_now - (instant_now - *instant);
        let epoch = approx
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");

        let epoch_ms = epoch.as_millis() as f64 / 1000.0;

        epoch_ms.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Instant, D::Error>
    where
        D: Deserializer<'de>,
    {
        let epoch_seconds: f64 = Deserialize::deserialize(deserializer)?;

        let since_epoch = Duration::from_secs_f64(epoch_seconds);

        let system_now = SystemTime::now();
        let instant_now = Instant::now();

        let deserialized_system_time = UNIX_EPOCH + since_epoch;

        let adjustment = match deserialized_system_time.duration_since(system_now) {
            Ok(duration) => -duration.as_secs_f64(),
            Err(e) => e.duration().as_secs_f64(),
        };

        let adjusted_instant = instant_now + Duration::from_secs_f64(adjustment);

        Ok(adjusted_instant)
    }
}
