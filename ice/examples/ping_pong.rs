use std::io;
use std::sync::Arc;
use std::time::Duration;

use clap::{App, AppSettings, Arg};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode};
use ice::agent::agent_config::AgentConfig;
use ice::agent::Agent;
use ice::candidate::candidate_base::*;
use ice::candidate::*;
use ice::network_type::*;
use ice::state::*;
use ice::udp_network::UDPNetwork;
use ice::Error;
use rand::{thread_rng, Rng};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, watch, Mutex};
use util::Conn;
use webrtc_ice as ice;

#[macro_use]
extern crate lazy_static;

type SenderType = Arc<Mutex<mpsc::Sender<String>>>;
type ReceiverType = Arc<Mutex<mpsc::Receiver<String>>>;

lazy_static! {
    // ErrUnknownType indicates an error with Unknown info.
    static ref REMOTE_AUTH_CHANNEL: (SenderType, ReceiverType ) = {
        let (tx, rx) = mpsc::channel::<String>(3);
        (Arc::new(Mutex::new(tx)), Arc::new(Mutex::new(rx)))
    };

    static ref REMOTE_CAND_CHANNEL: (SenderType, ReceiverType) = {
        let (tx, rx) = mpsc::channel::<String>(10);
        (Arc::new(Mutex::new(tx)), Arc::new(Mutex::new(rx)))
    };
}

// HTTP Listener to get ICE Credentials/Candidate from remote Peer
async fn remote_handler(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    //println!("received {:?}", req);
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/remoteAuth") => {
            let full_body =
                match std::str::from_utf8(&hyper::body::to_bytes(req.into_body()).await?) {
                    Ok(s) => s.to_owned(),
                    Err(err) => panic!("{}", err),
                };
            let tx = REMOTE_AUTH_CHANNEL.0.lock().await;
            //println!("body: {:?}", full_body);
            let _ = tx.send(full_body).await;

            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::OK;
            Ok(response)
        }

        (&Method::POST, "/remoteCandidate") => {
            let full_body =
                match std::str::from_utf8(&hyper::body::to_bytes(req.into_body()).await?) {
                    Ok(s) => s.to_owned(),
                    Err(err) => panic!("{}", err),
                };
            let tx = REMOTE_CAND_CHANNEL.0.lock().await;
            //println!("body: {:?}", full_body);
            let _ = tx.send(full_body).await;

            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::OK;
            Ok(response)
        }

        // Return the 404 Not Found for other routes.
        _ => {
            let mut not_found = Response::default();
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

// Controlled Agent:
//      cargo run --color=always --package webrtc-ice --example ping_pong
// Controlling Agent:
//      cargo run --color=always --package webrtc-ice --example ping_pong -- --controlling

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    // .format(|buf, record| {
    //     writeln!(
    //         buf,
    //         "{}:{} [{}] {} - {}",
    //         record.file().unwrap_or("unknown"),
    //         record.line().unwrap_or(0),
    //         record.level(),
    //         chrono::Local::now().format("%H:%M:%S.%6f"),
    //         record.args()
    //     )
    // })
    // .filter(None, log::LevelFilter::Trace)
    // .init();

    let mut app = App::new("ICE Demo")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of ICE")
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::SubcommandsNegateReqs)
        .arg(
            Arg::with_name("use-mux")
                .takes_value(false)
                .long("use-mux")
                .short('m')
                .help("Use a muxed UDP connection over a single listening port"),
        )
        .arg(
            Arg::with_name("FULLHELP")
                .help("Prints more detailed help information")
                .long("fullhelp"),
        )
        .arg(
            Arg::with_name("controlling")
                .takes_value(false)
                .long("controlling")
                .help("is ICE Agent controlling"),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let is_controlling = matches.is_present("controlling");
    let use_mux = matches.is_present("use-mux");

    let (local_http_port, remote_http_port) = if is_controlling {
        (9000, 9001)
    } else {
        (9001, 9000)
    };

    let (weak_conn, weak_agent) = {
        let (done_tx, done_rx) = watch::channel(());

        println!("Listening on http://localhost:{local_http_port}");
        let mut done_http_server = done_rx.clone();
        tokio::spawn(async move {
            let addr = ([0, 0, 0, 0], local_http_port).into();
            let service =
                make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(remote_handler)) });
            let server = Server::bind(&addr).serve(service);
            tokio::select! {
                _ = done_http_server.changed() => {
                    println!("receive cancel http server!");
                }
                result = server => {
                    // Run this server for... forever!
                    if let Err(e) = result {
                        eprintln!("server error: {e}");
                    }
                    println!("exit http server!");
                }
            };
        });

        if is_controlling {
            println!("Local Agent is controlling");
        } else {
            println!("Local Agent is controlled");
        };
        println!("Press 'Enter' when both processes have started");
        let mut input = String::new();
        let _ = io::stdin().read_line(&mut input)?;

        let udp_network = if use_mux {
            use ice::udp_mux::*;
            let port = if is_controlling { 4000 } else { 4001 };

            let udp_socket = UdpSocket::bind(("0.0.0.0", port)).await?;
            let udp_mux = UDPMuxDefault::new(UDPMuxParams::new(udp_socket));

            UDPNetwork::Muxed(udp_mux)
        } else {
            UDPNetwork::Ephemeral(Default::default())
        };

        let ice_agent = Arc::new(
            Agent::new(AgentConfig {
                network_types: vec![NetworkType::Udp4],
                udp_network,
                ..Default::default()
            })
            .await?,
        );

        let client = Arc::new(Client::new());

        // When we have gathered a new ICE Candidate send it to the remote peer
        let client2 = Arc::clone(&client);
        ice_agent.on_candidate(Box::new(
            move |c: Option<Arc<dyn Candidate + Send + Sync>>| {
                let client3 = Arc::clone(&client2);
                Box::pin(async move {
                    if let Some(c) = c {
                        println!("posting remoteCandidate with {}", c.marshal());

                        let req = match Request::builder()
                            .method(Method::POST)
                            .uri(format!(
                                "http://localhost:{remote_http_port}/remoteCandidate"
                            ))
                            .body(Body::from(c.marshal()))
                        {
                            Ok(req) => req,
                            Err(err) => {
                                println!("{err}");
                                return;
                            }
                        };
                        let resp = match client3.request(req).await {
                            Ok(resp) => resp,
                            Err(err) => {
                                println!("{err}");
                                return;
                            }
                        };
                        println!("Response from remoteCandidate: {}", resp.status());
                    }
                })
            },
        ));

        let (ice_done_tx, mut ice_done_rx) = mpsc::channel::<()>(1);
        // When ICE Connection state has change print to stdout
        ice_agent.on_connection_state_change(Box::new(move |c: ConnectionState| {
            println!("ICE Connection State has changed: {c}");
            if c == ConnectionState::Failed {
                let _ = ice_done_tx.try_send(());
            }
            Box::pin(async move {})
        }));

        // Get the local auth details and send to remote peer
        let (local_ufrag, local_pwd) = ice_agent.get_local_user_credentials().await;

        println!("posting remoteAuth with {local_ufrag}:{local_pwd}");
        let req = match Request::builder()
            .method(Method::POST)
            .uri(format!("http://localhost:{remote_http_port}/remoteAuth"))
            .body(Body::from(format!("{local_ufrag}:{local_pwd}")))
        {
            Ok(req) => req,
            Err(err) => return Err(Error::Other(format!("{err}"))),
        };
        let resp = match client.request(req).await {
            Ok(resp) => resp,
            Err(err) => return Err(Error::Other(format!("{err}"))),
        };
        println!("Response from remoteAuth: {}", resp.status());

        let (remote_ufrag, remote_pwd) = {
            let mut rx = REMOTE_AUTH_CHANNEL.1.lock().await;
            if let Some(s) = rx.recv().await {
                println!("received: {s}");
                let fields: Vec<String> = s.split(':').map(|s| s.to_string()).collect();
                (fields[0].clone(), fields[1].clone())
            } else {
                panic!("rx.recv() empty");
            }
        };
        println!("remote_ufrag: {remote_ufrag}, remote_pwd: {remote_pwd}");

        let ice_agent2 = Arc::clone(&ice_agent);
        let mut done_cand = done_rx.clone();
        tokio::spawn(async move {
            let mut rx = REMOTE_CAND_CHANNEL.1.lock().await;
            loop {
                tokio::select! {
                     _ = done_cand.changed() => {
                        println!("receive cancel remote cand!");
                        break;
                    }
                    result = rx.recv() => {
                        if let Some(s) = result {
                            if let Ok(c) = unmarshal_candidate(&s) {
                                println!("add_remote_candidate: {c}");
                                let c: Arc<dyn Candidate + Send + Sync> = Arc::new(c);
                                let _ = ice_agent2.add_remote_candidate(&c);
                            }else{
                                println!("unmarshal_candidate error!");
                                break;
                            }
                        }else{
                            println!("REMOTE_CAND_CHANNEL done!");
                            break;
                        }
                    }
                };
            }
        });

        ice_agent.gather_candidates()?;
        println!("Connecting...");

        let (_cancel_tx, cancel_rx) = mpsc::channel(1);
        // Start the ICE Agent. One side must be controlled, and the other must be controlling
        let conn: Arc<dyn Conn + Send + Sync> = if is_controlling {
            ice_agent.dial(cancel_rx, remote_ufrag, remote_pwd).await?
        } else {
            ice_agent
                .accept(cancel_rx, remote_ufrag, remote_pwd)
                .await?
        };

        let weak_conn = Arc::downgrade(&conn);

        // Send messages in a loop to the remote peer
        let conn_tx = Arc::clone(&conn);
        let mut done_send = done_rx.clone();
        tokio::spawn(async move {
            const RANDOM_STRING: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
            loop {
                tokio::time::sleep(Duration::from_secs(3)).await;

                let val: String = (0..15)
                    .map(|_| {
                        let idx = thread_rng().gen_range(0..RANDOM_STRING.len());
                        RANDOM_STRING[idx] as char
                    })
                    .collect();

                tokio::select! {
                     _ = done_send.changed() => {
                        println!("receive cancel ice send!");
                        break;
                    }
                    result = conn_tx.send(val.as_bytes()) => {
                        if let Err(err) = result {
                            eprintln!("conn_tx send error: {err}");
                            break;
                        }else{
                            println!("Sent: '{val}'");
                        }
                    }
                };
            }
        });

        let mut done_recv = done_rx.clone();
        tokio::spawn(async move {
            // Receive messages in a loop from the remote peer
            let mut buf = vec![0u8; 1500];
            loop {
                tokio::select! {
                    _ = done_recv.changed() => {
                        println!("receive cancel ice recv!");
                        break;
                    }
                    result = conn.recv(&mut buf) => {
                        match result {
                            Ok(n) => {
                                println!("Received: '{}'", std::str::from_utf8(&buf[..n]).unwrap());
                            }
                            Err(err) => {
                                eprintln!("conn_tx send error: {err}");
                                break;
                            }
                        };
                    }
                };
            }
        });

        println!("Press ctrl-c to stop");
        /*let d = if is_controlling {
            Duration::from_secs(500)
        } else {
            Duration::from_secs(5)
        };
        let timeout = tokio::time::sleep(d);
        tokio::pin!(timeout);*/

        tokio::select! {
            /*_ = timeout.as_mut() => {
                println!("received timeout signal!");
                let _ = done_tx.send(());
            }*/
            _ = ice_done_rx.recv() => {
                println!("ice_done_rx");
                let _ = done_tx.send(());
            }
            _ = tokio::signal::ctrl_c() => {
                println!();
                let _ = done_tx.send(());
            }
        };

        let _ = ice_agent.close().await;

        (weak_conn, Arc::downgrade(&ice_agent))
    };

    let mut int = tokio::time::interval(Duration::from_secs(1));
    loop {
        int.tick().await;
        println!(
            "weak_conn: weak count = {}, strong count = {}, weak_agent: weak count = {}, strong count = {}",
            weak_conn.weak_count(),
            weak_conn.strong_count(),
            weak_agent.weak_count(),
            weak_agent.strong_count(),
        );
        if weak_conn.strong_count() == 0 && weak_agent.strong_count() == 0 {
            break;
        }
    }

    Ok(())
}
