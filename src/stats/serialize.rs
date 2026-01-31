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

        let epoch_s = epoch.as_millis() as f64 / 1000.0;

        epoch_s.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Instant, D::Error>
    where
        D: Deserializer<'de>,
    {
        let epoch_s = f64::deserialize(deserializer)?;
        let epoch_duration = Duration::from_secs_f64(epoch_s);

        let system_now = SystemTime::now();
        let instant_now = Instant::now();

        let duration_since_approx = system_now
            .duration_since(UNIX_EPOCH + epoch_duration)
            .expect("Time went backwards");
        let instant = instant_now - duration_since_approx;

        Ok(instant)
    }
}
