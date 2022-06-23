#[cfg(test)]
macro_rules! test_serde_symmetry {
    (subject: $s:expr, json: $j:expr) => {
        // Serialize:
        {
            let actual = serde_json::to_value($s.clone()).unwrap();
            let expected = $j.clone();

            assert_eq!(actual, expected);
        }

        // Deserialize:
        {
            let actual: Subject = serde_json::from_value($j).unwrap();
            let expected = $s;

            assert_eq!(actual, expected);
        }
    };
}

#[cfg(test)]
pub(crate) use test_serde_symmetry;
