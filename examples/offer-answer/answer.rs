use anyhow::Result;
/*use clap::{App, AppSettings, Arg};
use interceptor::registry::Registry;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::peer::configuration::Configuration;
use webrtc::peer::ice::ice_candidate::ICECandidate;
use webrtc::peer::ice::ice_server::ICEServer;

#[macro_use]
extern crate lazy_static;*/

#[tokio::main]
async fn main() -> Result<()> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, log::LevelFilter::Trace)
    .init();*/
    /*
        let mut app = App::new("Answer")
            .version("0.1.0")
            .author("Rain Liu <yliu@webrtc.rs>")
            .about("An example of WebRTC-rs Answer")
            .setting(AppSettings::DeriveDisplayOrder)
            .setting(AppSettings::SubcommandsNegateReqs)
            .arg(
                Arg::with_name("FULLHELP")
                    .help("Prints more detailed help information")
                    .long("fullhelp"),
            )
            .arg(
                Arg::with_name("offer-address")
                    .required_unless("FULLHELP")
                    .takes_value(true)
                    .default_value("0.0.0.0:50000")
                    .long("offer-address")
                    .help("Address that the Offer HTTP server is hosted on."),
            )
            .arg(
                Arg::with_name("answer-address")
                    .required_unless("FULLHELP")
                    .takes_value(true)
                    .default_value("0.0.0.0:60000")
                    .long("answer-address")
                    .help("Address that the Answer HTTP server is hosted on."),
            );

        let matches = app.clone().get_matches();

        if matches.is_present("FULLHELP") {
            app.print_long_help().unwrap();
            std::process::exit(0);
        }

        let offerAddr = matches.value_of("offer-address").unwrap();
        let answerAddr = matches.value_of("answer-address").unwrap();

        //let mut pendingCandidates = vec![];

        // Prepare the configuration
        let config = Configuration {
            ice_servers: vec![ICEServer {
                urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                ..Default::default()
            }],
            ..Default::default()
        };

        // Create a MediaEngine object to configure the supported codec
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;

        let mut registry = Registry::new();

        // Use the default set of Interceptors
        registry = register_default_interceptors(registry, &mut m)?;

        // Create the API object with the MediaEngine
        let api = APIBuilder::new()
            .with_media_engine(m)
            .with_interceptor_registry(registry)
            .build();

        // Create a new RTCPeerConnection
        let mut peer_connection = api.new_peer_connection(config).await?;
    */
    /*
       // When an ICE candidate is available send to the other Pion instance
       // the other Pion instance will add this candidate by calling AddICECandidate
       peer_connection.on_ice_candidate(Box::new(|c: Option<ICECandidate>| {
           if c.is_none(){
               return Box::pin(async{});
           }

           candidatesMux.Lock()
           defer candidatesMux.Unlock()

           desc := peerConnection.RemoteDescription()
           if desc == nil {
               pendingCandidates = append(pendingCandidates, c)
           } else if onICECandidateErr := signalCandidate(*offerAddr, c); onICECandidateErr != nil {
               panic(onICECandidateErr)
           }
       })).await;

       // A HTTP handler that allows the other Pion instance to send us ICE candidates
       // This allows us to add ICE candidates faster, we don't have to wait for STUN or TURN
       // candidates which may be slower
       http.HandleFunc("/candidate", func(w http.ResponseWriter, r *http.Request) {
           candidate, candidateErr := ioutil.ReadAll(r.Body)
           if candidateErr != nil {
               panic(candidateErr)
           }
           if candidateErr := peerConnection.AddICECandidate(webrtc.ICECandidateInit{Candidate: string(candidate)}); candidateErr != nil {
               panic(candidateErr)
           }
       })

       // A HTTP handler that processes a SessionDescription given to us from the other Pion process
       http.HandleFunc("/sdp", func(w http.ResponseWriter, r *http.Request) {
           sdp := webrtc.SessionDescription{}
           if err := json.NewDecoder(r.Body).Decode(&sdp); err != nil {
               panic(err)
           }

           if err := peerConnection.SetRemoteDescription(sdp); err != nil {
               panic(err)
           }

           // Create an answer to send to the other process
           answer, err := peerConnection.CreateAnswer(nil)
           if err != nil {
               panic(err)
           }

           // Send our answer to the HTTP server listening in the other process
           payload, err := json.Marshal(answer)
           if err != nil {
               panic(err)
           }
           resp, err := http.Post(fmt.Sprintf("http://%s/sdp", *offerAddr), "application/json; charset=utf-8", bytes.NewReader(payload)) // nolint:noctx
           if err != nil {
               panic(err)
           } else if closeErr := resp.Body.Close(); closeErr != nil {
               panic(closeErr)
           }

           // Sets the LocalDescription, and starts our UDP listeners
           err = peerConnection.SetLocalDescription(answer)
           if err != nil {
               panic(err)
           }

           candidatesMux.Lock()
           for _, c := range pendingCandidates {
               onICECandidateErr := signalCandidate(*offerAddr, c)
               if onICECandidateErr != nil {
                   panic(onICECandidateErr)
               }
           }
           candidatesMux.Unlock()
       })

       // Set the handler for Peer connection state
       // This will notify you when the peer has connected/disconnected
       peerConnection.OnConnectionStateChange(func(s webrtc.PeerConnectionState) {
           fmt.Printf("Peer Connection State has changed: %s\n", s.String())

           if s == webrtc.PeerConnectionStateFailed {
               // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
               // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
               // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
               fmt.Println("Peer Connection has gone to failed exiting")
               os.Exit(0)
           }
       })

       // Register data channel creation handling
       peerConnection.OnDataChannel(func(d *webrtc.DataChannel) {
           fmt.Printf("New DataChannel %s %d\n", d.Label(), d.ID())

           // Register channel opening handling
           d.OnOpen(func() {
               fmt.Printf("Data channel '%s'-'%d' open. Random messages will now be sent to any connected DataChannels every 5 seconds\n", d.Label(), d.ID())

               for range time.NewTicker(5 * time.Second).C {
                   message := signal.RandSeq(15)
                   fmt.Printf("Sending '%s'\n", message)

                   // Send the message as text
                   sendTextErr := d.SendText(message)
                   if sendTextErr != nil {
                       panic(sendTextErr)
                   }
               }
           })

           // Register text message handling
           d.OnMessage(func(msg webrtc.DataChannelMessage) {
               fmt.Printf("Message from DataChannel '%s': '%s'\n", d.Label(), string(msg.Data))
           })
       })

       // Start HTTP server that accepts requests from the offer process to exchange SDP and Candidates
       panic(http.ListenAndServe(*answerAddr, nil))


       println!("Press ctlr-c to stop server");
       tokio::signal::ctrl_c().await.unwrap();

       peer_connection.close().await?;
    */
    Ok(())
}
