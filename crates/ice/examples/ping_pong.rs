use webrtc_ice as ice;

use ice::agent::{agent_config::*, *};
use ice::candidate::*;
use ice::network_type::*;
use ice::state::*;

use clap::{App, AppSettings, Arg};
use std::io;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use util::Error;

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode};

#[macro_use]
extern crate lazy_static;

lazy_static! {
    // ErrUnknownType indicates an error with Unknown info.
    static ref REMOTE_AUTH_CHANNEL: (Arc<Mutex<mpsc::Sender<String>>>, Arc<Mutex<mpsc::Receiver<String>>>) = {
        let (tx, rx) = mpsc::channel::<String>(3);
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
            let full_body = hyper::body::to_bytes(req.into_body()).await?;
            println!("body: {:?}", full_body);
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

    let mut app = App::new("ICE Demo")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of ICE")
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::SubcommandsNegateReqs)
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

    let (local_http_port, remote_http_port) = if is_controlling {
        (9000, 9001)
    } else {
        (9001, 9000)
    };

    println!("Listening on http://localhost:{}", local_http_port);
    tokio::spawn(async move {
        let addr = ([0, 0, 0, 0], local_http_port).into();
        let service =
            make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(remote_handler)) });
        let server = Server::bind(&addr).serve(service);
        // Run this server for... forever!
        if let Err(e) = server.await {
            eprintln!("server error: {}", e);
        }
    });

    if is_controlling {
        println!("Local Agent is controlling");
    } else {
        println!("Local Agent is controlled");
    };
    println!("Press 'Enter' when both processes have started");
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input)?;

    let ice_agent = Agent::new(AgentConfig {
        network_types: vec![NetworkType::Udp4],
        ..Default::default()
    })
    .await?;

    // When we have gathered a new ICE Candidate send it to the remote peer
    ice_agent
        .on_candidate(Box::new(
            move |c: Option<Arc<dyn Candidate + Send + Sync>>| {
                if c.is_none() {
                    return;
                }

                /*_, err = http.PostForm(fmt.Sprintf("http://localhost:%d/remoteCandidate", remoteHTTPPort), //nolint
                    url.Values{
                        "candidate": {c.Marshal()},
                    });
                */
            },
        ))
        .await;

    // When ICE Connection state has change print to stdout
    ice_agent
        .on_connection_state_change(Box::new(move |c: ConnectionState| {
            println!("ICE Connection State has changed: {}", c);
        }))
        .await;

    // Get the local auth details and send to remote peer
    let (local_ufrag, local_pwd) = ice_agent.get_local_user_credentials().await;

    let client = Client::new();
    let req = match Request::builder()
        .method(Method::POST)
        .uri(format!("http://localhost:{}/remoteAuth", remote_http_port))
        .header("content-type", "application/json")
        .body(Body::from(format!(
            "ufrag:{},pwd:{}",
            local_ufrag, local_pwd
        ))) {
        Ok(req) => req,
        Err(err) => return Err(Error::new(format!("{}", err))),
    };
    let resp = match client.request(req).await {
        Ok(resp) => resp,
        Err(err) => return Err(Error::new(format!("{}", err))),
    };
    println!("Response: {}", resp.status());

    let remote_str = {
        let mut rx = REMOTE_AUTH_CHANNEL.1.lock().await;
        if let Some(s) = rx.recv().await {
            s
        } else {
            panic!("rx.recv() empty");
        }
    };
    println!("Remote credentials: {}", remote_str);

    Ok(())
}
