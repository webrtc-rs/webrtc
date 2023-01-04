use crate::MediaTrackSettings;

pub(super) fn select_optimal_candidates<'a, I>(candidates: I) -> Vec<&'a MediaTrackSettings>
where
    I: IntoIterator<Item = (&'a MediaTrackSettings, f64)>,
{
    let mut optimal_candidates = vec![];
    let mut optimal_fitness_distance = f64::INFINITY;

    for (candidate, fitness_distance) in candidates {
        use std::cmp::Ordering;

        #[cfg(feature = "total_cmp")]
        let ordering = fitness_distance.total_cmp(&optimal_fitness_distance);

        // TODO: remove fallback once MSRV has been bumped to 1.62 or later:
        #[cfg(not(feature = "total_cmp"))]
        let ordering = {
            // See http://doc.rust-lang.org/1.65.0/core/primitive.f64.html#method.total_cmp:

            let mut left = fitness_distance.to_bits() as i64;
            let mut right = optimal_fitness_distance.to_bits() as i64;

            left ^= (((left >> 63) as u64) >> 1) as i64;
            right ^= (((right >> 63) as u64) >> 1) as i64;

            left.cmp(&right)
        };

        if ordering == Ordering::Less {
            // Candidate is new optimal, so drop current selection:
            optimal_candidates.clear();
            optimal_fitness_distance = fitness_distance;
        }

        if ordering != Ordering::Greater {
            // Candidate is optimal, so add to selection:
            optimal_candidates.push(candidate);
        }
    }

    optimal_candidates
}

#[cfg(test)]
mod tests {
    use crate::MediaTrackSettings;

    use super::select_optimal_candidates;

    #[test]
    fn monotonic_increasing() {
        let settings = vec![
            MediaTrackSettings::default(),
            MediaTrackSettings::default(),
            MediaTrackSettings::default(),
            MediaTrackSettings::default(),
        ];

        let candidates = vec![
            (&settings[0], 0.1),
            (&settings[1], 0.1),
            (&settings[2], 0.2),
            (&settings[3], 0.3),
        ];

        let actual = select_optimal_candidates(candidates);

        let expected = vec![&settings[0], &settings[1]];

        assert_eq!(actual, expected);
    }

    #[test]
    fn monotonic_decreasing() {
        let settings = vec![
            MediaTrackSettings::default(),
            MediaTrackSettings::default(),
            MediaTrackSettings::default(),
            MediaTrackSettings::default(),
        ];

        let candidates = vec![
            (&settings[0], 0.3),
            (&settings[1], 0.2),
            (&settings[2], 0.1),
            (&settings[3], 0.1),
        ];

        let actual = select_optimal_candidates(candidates);

        let expected = vec![&settings[2], &settings[3]];

        assert_eq!(actual, expected);
    }

    #[test]
    fn alternating() {
        let settings = vec![
            MediaTrackSettings::default(),
            MediaTrackSettings::default(),
            MediaTrackSettings::default(),
            MediaTrackSettings::default(),
        ];

        let candidates = vec![
            (&settings[0], 0.2),
            (&settings[1], 0.1),
            (&settings[2], 0.2),
            (&settings[3], 0.1),
        ];

        let actual = select_optimal_candidates(candidates);

        let expected = vec![&settings[1], &settings[3]];

        assert_eq!(actual, expected);
    }
}
