use super::*;

use util::Error;

use tokio::sync::{mpsc, Mutex};
use tokio::time::{delay_for, Duration};

use std::sync::Arc;

#[tokio::test]
async fn test_deadline() -> Result<(), Error> {
    let (tx, mut rx) = mpsc::channel(3);
    let d = Deadline::new().await;
    d.set(Duration::from_millis(50)).await;

    let txs = Arc::new(Mutex::new(tx));
    let tx0 = Arc::clone(&txs);
    let tx1 = Arc::clone(&txs);
    let tx2 = Arc::clone(&txs);
    tokio::spawn(async move {
        delay_for(Duration::from_millis(40)).await;
        let mut tx = tx0.lock().await;
        let _ = tx.send(0).await;
    });

    tokio::spawn(async move {
        delay_for(Duration::from_millis(60)).await;
        let mut tx = tx1.lock().await;
        let _ = tx.send(1).await;
    });

    tokio::spawn(async move {
        d.done().await;
        let mut tx = tx2.lock().await;
        let _ = tx.send(2).await;
    });

    let expected_calls = vec![0, 2, 1];
    let mut timeout_100ms = delay_for(Duration::from_millis(100));
    let mut calls = vec![];
    for _ in 0..expected_calls.len() {
        tokio::select! {
            call = rx.recv() =>{
                if let Some(call) = call{
                    calls.push(call);
                }
            }
            _= &mut timeout_100ms => {
                break;
            }
        }
    }

    assert_eq!(calls, expected_calls);

    Ok(())
}

#[tokio::test]
async fn test_deadline_extend() -> Result<(), Error> {
    let (tx, mut rx) = mpsc::channel(3);
    let d = Deadline::new().await;
    d.set(Duration::from_millis(50)).await;
    d.set(Duration::from_millis(70)).await;

    let txs = Arc::new(Mutex::new(tx));
    let tx0 = Arc::clone(&txs);
    let tx1 = Arc::clone(&txs);
    let tx2 = Arc::clone(&txs);
    tokio::spawn(async move {
        delay_for(Duration::from_millis(40)).await;
        let mut tx = tx0.lock().await;
        let _ = tx.send(0).await;
    });

    tokio::spawn(async move {
        delay_for(Duration::from_millis(60)).await;
        let mut tx = tx1.lock().await;
        let _ = tx.send(1).await;
    });

    tokio::spawn(async move {
        d.done().await;
        let mut tx = tx2.lock().await;
        let _ = tx.send(2).await;
    });

    let expected_calls = vec![0, 1, 2];
    let mut timeout_100ms = delay_for(Duration::from_millis(100));
    let mut calls = vec![];
    for _ in 0..expected_calls.len() {
        tokio::select! {
            call = rx.recv() =>{
                if let Some(call) = call{
                    calls.push(call);
                }
            }
            _= &mut timeout_100ms => {
                break;
            }
        }
    }

    assert_eq!(calls, expected_calls);

    Ok(())
}

#[tokio::test]
async fn test_deadline_pretend() -> Result<(), Error> {
    let (tx, mut rx) = mpsc::channel(3);
    let d = Deadline::new().await;
    d.set(Duration::from_millis(50)).await;
    d.set(Duration::from_millis(30)).await;

    let txs = Arc::new(Mutex::new(tx));
    let tx0 = Arc::clone(&txs);
    let tx1 = Arc::clone(&txs);
    let tx2 = Arc::clone(&txs);
    tokio::spawn(async move {
        delay_for(Duration::from_millis(40)).await;
        let mut tx = tx0.lock().await;
        let _ = tx.send(0).await;
    });

    tokio::spawn(async move {
        delay_for(Duration::from_millis(60)).await;
        let mut tx = tx1.lock().await;
        let _ = tx.send(1).await;
    });

    tokio::spawn(async move {
        d.done().await;
        let mut tx = tx2.lock().await;
        let _ = tx.send(2).await;
    });

    let expected_calls = vec![2, 0, 1];
    let mut timeout_100ms = delay_for(Duration::from_millis(100));
    let mut calls = vec![];
    for _ in 0..expected_calls.len() {
        tokio::select! {
            call = rx.recv() =>{
                if let Some(call) = call{
                    calls.push(call);
                }
            }
            _= &mut timeout_100ms => {
                break;
            }
        }
    }

    assert_eq!(calls, expected_calls);

    Ok(())
}

#[tokio::test]
async fn test_deadline_cancel() -> Result<(), Error> {
    let (tx, mut rx) = mpsc::channel(3);
    let d = Deadline::new().await;
    d.set(Duration::from_millis(50)).await;
    d.set(Duration::from_millis(0)).await;

    let txs = Arc::new(Mutex::new(tx));
    let tx0 = Arc::clone(&txs);
    let tx1 = Arc::clone(&txs);
    let tx2 = Arc::clone(&txs);
    tokio::spawn(async move {
        delay_for(Duration::from_millis(40)).await;
        let mut tx = tx0.lock().await;
        let _ = tx.send(0).await;
    });

    tokio::spawn(async move {
        delay_for(Duration::from_millis(60)).await;
        let mut tx = tx1.lock().await;
        let _ = tx.send(1).await;
    });

    tokio::spawn(async move {
        d.done().await;
        let mut tx = tx2.lock().await;
        let _ = tx.send(2).await;
    });

    let expected_calls = vec![0, 1];
    let mut timeout_100ms = delay_for(Duration::from_millis(100));
    let mut calls = vec![];
    for _ in 0..expected_calls.len() {
        tokio::select! {
            call = rx.recv() =>{
                if let Some(call) = call{
                    calls.push(call);
                }
            }
            _= &mut timeout_100ms => {
                break;
            }
        }
    }

    assert_eq!(calls, expected_calls);

    Ok(())
}
