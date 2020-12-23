use super::*;
use crate::errors::*;

use std::ops::Add;
use tokio::time::Duration;

use util::Error;

#[tokio::test]
async fn test_agent_process_in_transaction() -> Result<(), Error> {
    let m = Arc::new(Mutex::new(Message::new()));
    let (handler_tx, mut hander_rx) = tokio::sync::mpsc::unbounded_channel();

    let mut a = Agent::new(Some(handler_tx));
    let transaction_id;
    {
        let mut msg = m.lock().await;
        msg.transaction_id = TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        transaction_id = msg.transaction_id;
    }
    a.start(transaction_id, Instant::now())?;
    a.process(transaction_id, Some(m))?;
    a.close()?;

    while let Some(e) = hander_rx.recv().await {
        assert!(
            e.error.is_none(),
            "got error: {}",
            e.error.as_ref().unwrap()
        );
        if let Some(msg) = &e.message {
            let m2 = msg.lock().await;
            let tid = TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
            assert_eq!(
                m2.transaction_id, tid,
                "{:?} (got) != {:?} (expected)",
                m2.transaction_id, tid
            );
        } else {
            assert!(false, "expected error, but got ok");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_agent_process() -> Result<(), Error> {
    let m = Arc::new(Mutex::new(Message::new()));
    let (handler_tx, mut hander_rx) = tokio::sync::mpsc::unbounded_channel();

    let mut a = Agent::new(Some(handler_tx));

    let transaction_id;
    {
        let mut msg = m.lock().await;
        msg.transaction_id = TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        transaction_id = msg.transaction_id;
    }

    a.process(transaction_id, Some(Arc::clone(&m)))?;
    a.close()?;

    while let Some(e) = hander_rx.recv().await {
        assert!(
            e.error.is_none(),
            "got error: {}",
            e.error.as_ref().unwrap()
        );
        if let Some(msg) = &e.message {
            let m2 = msg.lock().await;
            let tid = TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
            assert_eq!(
                m2.transaction_id, tid,
                "{:?} (got) != {:?} (expected)",
                m2.transaction_id, tid
            );
        } else {
            assert!(false, "expected error, but got ok");
        }
    }

    let result = a.process(transaction_id, Some(m));
    if let Err(err) = result {
        assert_eq!(
            err,
            ERR_AGENT_CLOSED.clone(),
            "closed agent should return <{}>, but got <{}>",
            ERR_AGENT_CLOSED.clone(),
            err,
        );
    } else {
        assert!(false, "expected error, but got ok");
    }

    Ok(())
}

#[test]
fn test_agent_start() -> Result<(), Error> {
    let mut a = Agent::new(noop_handler());
    let id = TransactionId::new();
    let deadline = Instant::now().add(Duration::from_secs(3600));
    a.start(id, deadline)?;

    let result = a.start(id, deadline);
    if let Err(err) = result {
        assert_eq!(
            err,
            ERR_TRANSACTION_EXISTS.clone(),
            "duplicate start should return <{}>, got <{}>",
            ERR_TRANSACTION_EXISTS.clone(),
            err,
        );
    } else {
        assert!(false, "expected error, but got ok");
    }
    a.close()?;

    let id = TransactionId::new();
    let result = a.start(id, deadline);
    if let Err(err) = result {
        assert_eq!(
            err,
            ERR_AGENT_CLOSED.clone(),
            "start on closed agent should return <{}>, got <{}>",
            ERR_AGENT_CLOSED.clone(),
            err,
        );
    } else {
        assert!(false, "expected error, but got ok");
    }

    let result = a.set_handler(noop_handler());
    if let Err(err) = result {
        assert_eq!(
            err,
            ERR_AGENT_CLOSED.clone(),
            "SetHandler on closed agent should return <{}>, got <{}>",
            ERR_AGENT_CLOSED.clone(),
            err,
        );
    } else {
        assert!(false, "expected error, but got ok");
    }

    Ok(())
}

#[tokio::test]
async fn test_agent_stop() -> Result<(), Error> {
    let (handler_tx, mut hander_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut a = Agent::new(Some(handler_tx));

    let result = a.stop(TransactionId::default());
    if let Err(err) = result {
        assert_eq!(
            err,
            ERR_TRANSACTION_NOT_EXISTS.clone(),
            "unexpected error: {}, should be {}",
            ERR_TRANSACTION_NOT_EXISTS.clone(),
            err,
        );
    } else {
        assert!(false, "expected error, but got ok");
    }

    let id = TransactionId::new();
    let deadline = Instant::now().add(Duration::from_millis(200));
    a.start(id, deadline)?;
    a.stop(id)?;

    let timeout = tokio::time::sleep(Duration::from_millis(400));
    tokio::pin!(timeout);

    tokio::select! {
        evt = hander_rx.recv() => {
            if let Some(error) = evt.unwrap().error{
                assert_eq!(error, ERR_TRANSACTION_STOPPED.clone(),
                    "unexpected error: {}, should be {}",
                    error, ERR_TRANSACTION_STOPPED.clone());
            }else{
                assert!(false, "expected error, got ok");
            }
        }
     _ = timeout.as_mut() => assert!(false, "timed out"),
    }

    a.close()?;

    let result = a.close();
    if let Err(err) = result {
        assert_eq!(
            err,
            ERR_AGENT_CLOSED.clone(),
            "a.Close returned {} instead of {}",
            ERR_AGENT_CLOSED.clone(),
            err,
        );
    } else {
        assert!(false, "expected error, but got ok");
    }

    let result = a.stop(TransactionId::default());
    if let Err(err) = result {
        assert_eq!(
            err,
            ERR_AGENT_CLOSED.clone(),
            "unexpected error: {}, should be {}",
            ERR_AGENT_CLOSED.clone(),
            err,
        );
    } else {
        assert!(false, "expected error, but got ok");
    }

    Ok(())
}
