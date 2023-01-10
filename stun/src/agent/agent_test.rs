use super::*;
use crate::error::*;

use std::ops::Add;
use tokio::time::Duration;

#[tokio::test]
async fn test_agent_process_in_transaction() -> Result<()> {
    let mut m = Message::new();
    let (handler_tx, mut handler_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut a = Agent::new(Some(Arc::new(handler_tx)));
    m.transaction_id = TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    a.start(m.transaction_id, Instant::now())?;
    a.process(m)?;
    a.close()?;

    while let Some(e) = handler_rx.recv().await {
        assert!(e.event_body.is_ok(), "got error: {:?}", e.event_body);

        let tid = TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(
            e.event_body.as_ref().unwrap().transaction_id,
            tid,
            "{:?} (got) != {:?} (expected)",
            e.event_body.as_ref().unwrap().transaction_id,
            tid
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_agent_process() -> Result<()> {
    let mut m = Message::new();
    let (handler_tx, mut handler_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut a = Agent::new(Some(Arc::new(handler_tx)));
    m.transaction_id = TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    a.process(m.clone())?;
    a.close()?;

    while let Some(e) = handler_rx.recv().await {
        assert!(e.event_body.is_ok(), "got error: {:?}", e.event_body);

        let tid = TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(
            e.event_body.as_ref().unwrap().transaction_id,
            tid,
            "{:?} (got) != {:?} (expected)",
            e.event_body.as_ref().unwrap().transaction_id,
            tid
        );
    }

    let result = a.process(m);
    if let Err(err) = result {
        assert_eq!(
            err,
            Error::ErrAgentClosed,
            "closed agent should return <{}>, but got <{}>",
            Error::ErrAgentClosed,
            err,
        );
    } else {
        panic!("expected error, but got ok");
    }

    Ok(())
}

#[test]
fn test_agent_start() -> Result<()> {
    let mut a = Agent::new(noop_handler());
    let id = TransactionId::new();
    let deadline = Instant::now().add(Duration::from_secs(3600));
    a.start(id, deadline)?;

    let result = a.start(id, deadline);
    if let Err(err) = result {
        assert_eq!(
            err,
            Error::ErrTransactionExists,
            "duplicate start should return <{}>, got <{}>",
            Error::ErrTransactionExists,
            err,
        );
    } else {
        panic!("expected error, but got ok");
    }
    a.close()?;

    let id = TransactionId::new();
    let result = a.start(id, deadline);
    if let Err(err) = result {
        assert_eq!(
            err,
            Error::ErrAgentClosed,
            "start on closed agent should return <{}>, got <{}>",
            Error::ErrAgentClosed,
            err,
        );
    } else {
        panic!("expected error, but got ok");
    }

    let result = a.set_handler(noop_handler());
    if let Err(err) = result {
        assert_eq!(
            err,
            Error::ErrAgentClosed,
            "SetHandler on closed agent should return <{}>, got <{}>",
            Error::ErrAgentClosed,
            err,
        );
    } else {
        panic!("expected error, but got ok");
    }

    Ok(())
}

#[tokio::test]
async fn test_agent_stop() -> Result<()> {
    let (handler_tx, mut handler_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut a = Agent::new(Some(Arc::new(handler_tx)));

    let result = a.stop(TransactionId::default());
    if let Err(err) = result {
        assert_eq!(
            err,
            Error::ErrTransactionNotExists,
            "unexpected error: {}, should be {}",
            Error::ErrTransactionNotExists,
            err,
        );
    } else {
        panic!("expected error, but got ok");
    }

    let id = TransactionId::new();
    let deadline = Instant::now().add(Duration::from_millis(200));
    a.start(id, deadline)?;
    a.stop(id)?;

    let timeout = tokio::time::sleep(Duration::from_millis(400));
    tokio::pin!(timeout);

    tokio::select! {
        evt = handler_rx.recv() => {
            if let Err(err) = evt.unwrap().event_body{
                assert_eq!(
                    err,
                    Error::ErrTransactionStopped,
                    "unexpected error: {}, should be {}",
                    err,
                    Error::ErrTransactionStopped
                );
            }else{
                panic!("expected error, got ok");
            }
        }
     _ = timeout.as_mut() => panic!("timed out"),
    }

    a.close()?;

    let result = a.close();
    if let Err(err) = result {
        assert_eq!(
            err,
            Error::ErrAgentClosed,
            "a.Close returned {} instead of {}",
            Error::ErrAgentClosed,
            err,
        );
    } else {
        panic!("expected error, but got ok");
    }

    let result = a.stop(TransactionId::default());
    if let Err(err) = result {
        assert_eq!(
            err,
            Error::ErrAgentClosed,
            "unexpected error: {}, should be {}",
            Error::ErrAgentClosed,
            err,
        );
    } else {
        panic!("expected error, but got ok");
    }

    Ok(())
}
