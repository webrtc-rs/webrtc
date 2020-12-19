use super::*;
use crate::errors::*;

//use tokio::sync::mpsc;
use tokio::time::Duration;

use util::Error;

#[test]
fn test_agent_process_in_transaction() -> Result<(), Error> {
    let m = Rc::new(RefCell::new(Message::new()));
    let m2 = Rc::clone(&m);
    let mut a = Agent::new(Box::new(move |e| {
        assert!(
            e.error.is_none(),
            "got error: {}",
            e.error.as_ref().unwrap()
        );
        assert_eq!(
            e.message, m2,
            "{:?} (got) != {:?} (expected)",
            e.message, m2
        );
    }));

    {
        let mut msg = m.borrow_mut();
        msg.new_transaction_id()?;
    }
    a.start(m.borrow().transaction_id, Duration::from_millis(0))?;
    a.process(&m)?;
    a.close()?;

    Ok(())
}

#[test]
fn test_agent_process() -> Result<(), Error> {
    let m = Rc::new(RefCell::new(Message::new()));
    let m2 = Rc::clone(&m);
    let mut a = Agent::new(Box::new(move |e| {
        assert!(
            e.error.is_none(),
            "got error: {}",
            e.error.as_ref().unwrap()
        );
        assert_eq!(
            e.message, m2,
            "{:?} (got) != {:?} (expected)",
            e.message, m2
        );
    }));

    {
        let mut msg = m.borrow_mut();
        msg.new_transaction_id()?;
    }

    a.process(&m)?;
    a.close()?;

    let result = a.process(&m);
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
    let deadline = Duration::from_secs(3600);
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

/*TODO:
#[tokio::test]
async fn test_agent_stop() -> Result<(), Error> {
    let (called_tx, mut called_rx) = mpsc::channel(8);

    let mut a = Agent::new(Box::new(move |e| {
        let _ = called_tx.send(e.clone()); //.await;
    }));

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
    let deadline = Duration::from_millis(200);
    a.start(id, deadline)?;
    a.stop(id)?;

    let mut timeout = tokio::time::sleep(Duration::from_millis(400));

    tokio::select! {
        evt = called_rx.recv() => {
            if let Some(error) = evt.unwrap().error{
                assert_eq!(error, ERR_TRANSACTION_STOPPED.clone(),
                    "unexpected error: {}, should be {}",
                    error, ERR_TRANSACTION_STOPPED.clone());
            }else{
                assert!(false, "expected error, got ok");
            }
        }
     _ = &mut timeout => assert!(false, "timed out"),
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
 */
