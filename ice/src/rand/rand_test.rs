use super::*;
use crate::error::Result;

use std::sync::Arc;
use tokio::sync::Mutex;
use waitgroup::WaitGroup;

#[tokio::test]
async fn test_random_generator_collision() -> Result<()> {
    let test_cases = vec![
        (
            "CandidateID",
            0, /*||-> String {
                   generate_cand_id()
               },*/
        ),
        (
            "PWD", 1, /*||-> String {
                  generate_pwd()
              },*/
        ),
        (
            "Ufrag", 2, /*|| ->String {
                  generate_ufrag()
              },*/
        ),
    ];

    const N: usize = 10;
    const ITERATION: usize = 10;

    for (name, test_case) in test_cases {
        for _ in 0..ITERATION {
            let rands = Arc::new(Mutex::new(vec![]));

            // Create a new wait group.
            let wg = WaitGroup::new();

            for _ in 0..N {
                let w = wg.worker();
                let rs = Arc::clone(&rands);

                tokio::spawn(async move {
                    let _d = w;

                    let s = if test_case == 0 {
                        generate_cand_id()
                    } else if test_case == 1 {
                        generate_pwd()
                    } else {
                        generate_ufrag()
                    };

                    let mut r = rs.lock().await;
                    r.push(s);
                });
            }
            wg.wait().await;

            let rs = rands.lock().await;
            assert_eq!(rs.len(), N, "{name} Failed to generate randoms");

            for i in 0..N {
                for j in i + 1..N {
                    assert_ne!(
                        rs[i], rs[j],
                        "{}: generateRandString caused collision: {} == {}",
                        name, rs[i], rs[j],
                    );
                }
            }
        }
    }

    Ok(())
}
