use super::*;
use crate::error::Error;

use std::io::{BufReader, BufWriter};

#[test]
fn test_alert() -> Result<()> {
    let tests = vec![
        (
            "Valid Alert",
            vec![0x02, 0x0A],
            Alert {
                alert_level: AlertLevel::Fatal,
                alert_description: AlertDescription::UnexpectedMessage,
            },
            None,
        ),
        (
            "Invalid alert length",
            vec![0x00],
            Alert {
                alert_level: AlertLevel::Invalid,
                alert_description: AlertDescription::Invalid,
            },
            Some(Error::Other("io".to_owned())),
        ),
    ];

    for (name, data, wanted, unmarshal_error) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let result = Alert::unmarshal(&mut reader);

        if let Some(err) = unmarshal_error {
            assert!(result.is_err(), "{} expected error: {}", name, err);
        } else if let Ok(alert) = result {
            assert_eq!(
                wanted, alert,
                "{} expected {}, but got {}",
                name, wanted, alert
            );

            let mut data2: Vec<u8> = vec![];
            {
                let mut writer = BufWriter::<&mut Vec<u8>>::new(data2.as_mut());
                alert.marshal(&mut writer)?;
            }
            assert_eq!(
                data, data2,
                "{} expected {:?}, but got {:?}",
                name, data, data2
            );
        } else {
            assert!(result.is_ok(), "{} expected Ok, but has error", name);
        }
    }

    Ok(())
}
