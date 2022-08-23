use super::*;
use crate::error::Result;

#[test]
fn test_connected_state_string() -> Result<()> {
    let tests = vec![
        (ConnectionState::Unspecified, "Unspecified"),
        (ConnectionState::New, "New"),
        (ConnectionState::Checking, "Checking"),
        (ConnectionState::Connected, "Connected"),
        (ConnectionState::Completed, "Completed"),
        (ConnectionState::Failed, "Failed"),
        (ConnectionState::Disconnected, "Disconnected"),
        (ConnectionState::Closed, "Closed"),
    ];

    for (connection_state, expected_string) in tests {
        assert_eq!(
            expected_string,
            connection_state.to_string(),
            "testCase: {} vs {}",
            expected_string,
            connection_state,
        )
    }

    Ok(())
}

#[test]
fn test_gathering_state_string() -> Result<()> {
    let tests = vec![
        (GatheringState::Unspecified, "unspecified"),
        (GatheringState::New, "new"),
        (GatheringState::Gathering, "gathering"),
        (GatheringState::Complete, "complete"),
    ];

    for (gathering_state, expected_string) in tests {
        assert_eq!(
            expected_string,
            gathering_state.to_string(),
            "testCase: {} vs {}",
            expected_string,
            gathering_state,
        )
    }

    Ok(())
}
