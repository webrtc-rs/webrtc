use super::*;

use crossbeam::sync::WaitGroup;
use std::sync::{Arc, Mutex};
use std::thread;
use util::Error;

#[test]
fn test_random_generator_collision() -> Result<(), Error> {
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
                let wg = wg.clone();
                let rs = Arc::clone(&rands);

                thread::spawn(move || {
                    let s = if test_case == 0 {
                        generate_cand_id()
                    } else if test_case == 1 {
                        generate_pwd()
                    } else {
                        generate_ufrag()
                    };

                    if let Ok(mut r) = rs.lock() {
                        r.push(s);
                    }

                    drop(wg);
                });
            }
            wg.wait();

            if let Ok(rs) = rands.lock() {
                assert_eq!(rs.len(), N, "{} Failed to generate randoms", name);

                for i in 0..N {
                    for j in i + 1..N {
                        assert_ne!(
                            rs[i], rs[j],
                            "{}: generateRandString caused collision: {} == {}",
                            name, rs[i], rs[j],
                        );
                    }
                }
            };
        }
    }

    Ok(())
}
