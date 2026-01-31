use tokio::sync::Mutex;

use super::*;
use crate::error::Result;

#[tokio::test]
async fn test_operations_enqueue() -> Result<()> {
    let ops = Operations::new();
    for _ in 0..100 {
        let results = Arc::new(Mutex::new(vec![0; 16]));
        for k in 0..16 {
            let r = Arc::clone(&results);
            ops.enqueue(Operation::new(
                move || {
                    let r2 = Arc::clone(&r);
                    Box::pin(async move {
                        let mut r3 = r2.lock().await;
                        r3[k] += k * k;
                        r3[k] == 225
                    })
                },
                "test_operations_enqueue",
            ))
            .await?;
        }

        ops.done().await;
        let expected = vec![
            0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100, 121, 144, 169, 196, 450,
        ];
        {
            let r = results.lock().await;
            assert_eq!(r.len(), expected.len());
            assert_eq!(&*r, &expected);
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_operations_done() -> Result<()> {
    let ops = Operations::new();
    ops.done().await;

    Ok(())
}
