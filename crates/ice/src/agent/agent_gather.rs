use super::*;
use crate::errors::*;
use crate::network_type::*;
use crate::url::{ProtoType, SchemeType, URL};
use crate::util::*;

use util::{Conn, Error};

use crate::candidate::candidate_base::CandidateBaseConfig;
use crate::candidate::candidate_host::CandidateHostConfig;
use crate::candidate::candidate_relay::CandidateRelayConfig;
use crate::candidate::candidate_server_reflexive::CandidateServerReflexiveConfig;
use crate::candidate::*;
use defer::defer;
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use waitgroup::WaitGroup;

const STUN_GATHER_TIMEOUT: Duration = Duration::from_secs(5);

impl Agent {
    fn set_gathering_state(&self, new_state: GatheringState) {
        if GatheringState::from(self.gathering_state.load(Ordering::SeqCst)) != new_state
            && new_state == GatheringState::Complete
        {
            //TODO: a.chanCandidate <- nil
        }

        self.gathering_state
            .store(new_state as u8, Ordering::SeqCst);
    }

    pub(crate) fn gather_candidates(&self) {
        self.set_gathering_state(GatheringState::Gathering);
        /*
        var wg sync.WaitGroup
        for _, t := range a.candidateTypes {
            switch t {
            case CandidateTypeHost:
                wg.Add(1)
                go func() {
                    a.gather_candidates_local(ctx, a.networkTypes)
                    wg.Done()
                }()
            case CandidateTypeServerReflexive:
                wg.Add(1)
                go func() {
                    a.gather_candidates_srflx(ctx, a.urls, a.networkTypes)
                    wg.Done()
                }()
                if a.extIPMapper != nil && a.extIPMapper.candidateType == CandidateTypeServerReflexive {
                    wg.Add(1)
                    go func() {
                        a.gather_candidates_srflx_mapped(ctx, a.networkTypes)
                        wg.Done()
                    }()
                }
            case CandidateTypeRelay:
                wg.Add(1)
                go func() {
                    a.gather_candidates_relay(ctx, a.urls)
                    wg.Done()
                }()
            case CandidateTypePeerReflexive, CandidateTypeUnspecified:
            }
        }
        // Block until all STUN and TURN URLs have been gathered (or timed out)
        wg.Wait()
        */
        self.set_gathering_state(GatheringState::Complete);
    }

    pub(crate) async fn gather_candidates_local(&self, network_types: Vec<NetworkType>) {
        let (port_max, port_min) = (self.port_max, self.port_min);

        let local_ips = match local_interfaces(&self.interface_filter, &network_types) {
            Ok(ips) => ips,
            Err(err) => {
                log::warn!(
                    "failed to iterate local interfaces, host candidates will not be gathered {}",
                    err
                );
                return;
            }
        };

        for ip in local_ips {
            let mut mapped_ip = ip;

            {
                let ai = self.agent_internal.lock().await;
                if self.mdns_mode != MulticastDNSMode::QueryAndGather
                    && ai.ext_ip_mapper.candidate_type == CandidateType::Host
                {
                    if let Ok(mi) = ai.ext_ip_mapper.find_external_ip(&ip.to_string()) {
                        mapped_ip = mi;
                    } else {
                        log::warn!(
                            "1:1 NAT mapping is enabled but no external IP is found for {}",
                            ip
                        );
                    }
                }
            }

            let address = if self.mdns_mode == MulticastDNSMode::QueryAndGather {
                self.mdns_name.clone()
            } else {
                mapped_ip.to_string()
            };

            //TODO: for network in networks
            let network = UDP.to_owned();
            {
                /*TODO:switch network {
                case tcp:
                    // Handle ICE TCP passive mode

                    a.log.Debugf("GetConn by ufrag: %s\n", a.localUfrag)
                    conn, err = a.tcpMux.GetConnByUfrag(a.localUfrag)
                    if err != nil {
                        if !errors.Is(err, ErrTCPMuxNotInitialized) {
                            a.log.Warnf("error getting tcp conn by ufrag: %s %s %s\n", network, ip, a.localUfrag)
                        }
                        continue
                    }
                    port = conn.LocalAddr().(*net.TCPAddr).Port
                    tcpType = TCPTypePassive
                    // is there a way to verify that the listen address is even
                    // accessible from the current interface.
                case udp:*/

                let conn: Arc<dyn Conn + Send + Sync> = match listen_udp_in_port_range(
                    port_max,
                    port_min,
                    SocketAddr::new(ip, 0),
                )
                .await
                {
                    Ok(conn) => Arc::new(conn),
                    Err(err) => {
                        log::warn!("could not listen {} {}: {}", network, ip, err);
                        continue;
                    }
                };

                let port = match conn.local_addr() {
                    Ok(addr) => addr.port(),
                    Err(err) => {
                        log::warn!("could not get local addr: {}", err);
                        continue;
                    }
                };

                let host_config = CandidateHostConfig {
                    base_config: CandidateBaseConfig {
                        network: network.clone(),
                        address,
                        port,
                        component: COMPONENT_RTP,
                        conn: Some(conn),
                        ..Default::default()
                    },
                    ..Default::default()
                };

                let candidate: Arc<dyn Candidate + Send + Sync> =
                    match host_config.new_candidate_host().await {
                        Ok(mut candidate) => {
                            if self.mdns_mode == MulticastDNSMode::QueryAndGather {
                                if let Err(err) = candidate.set_ip(&ip) {
                                    log::warn!(
                                        "Failed to create host candidate: {} {} {}: {}",
                                        network,
                                        mapped_ip,
                                        port,
                                        err
                                    );
                                    continue;
                                }
                            }
                            Arc::new(candidate)
                        }
                        Err(err) => {
                            log::warn!(
                                "Failed to create host candidate: {} {} {}: {}",
                                network,
                                mapped_ip,
                                port,
                                err
                            );
                            continue;
                        }
                    };

                {
                    let mut ai = self.agent_internal.lock().await;
                    if let Err(err) = ai.add_candidate(&candidate).await {
                        if let Err(close_err) = candidate.close().await {
                            log::warn!("Failed to close candidate: {}", close_err);
                        }
                        log::warn!(
                            "Failed to append to localCandidates and run onCandidateHdlr: {}",
                            err
                        );
                    }
                }
            }
        }
    }

    pub(crate) async fn gather_candidates_srflx_mapped(&self, network_types: Vec<NetworkType>) {
        let (port_max, port_min) = (self.port_max, self.port_min);

        let wg = WaitGroup::new();

        for network_type in network_types {
            if network_type.is_tcp() {
                continue;
            }

            let w = wg.worker();
            let network = network_type.to_string();
            let agent_internal = Arc::clone(&self.agent_internal);

            tokio::spawn(async move {
                let _d = defer(move || {
                    drop(w);
                });

                let conn: Arc<dyn Conn + Send + Sync> = match listen_udp_in_port_range(
                    port_max,
                    port_min,
                    SocketAddr::from_str("0.0.0.0:0")?,
                )
                .await
                {
                    Ok(conn) => Arc::new(conn),
                    Err(err) => {
                        log::warn!("Failed to listen {}: {}", network, err);
                        return Ok(());
                    }
                };

                let laddr = conn.local_addr()?;
                let mapped_ip = {
                    let ai = agent_internal.lock().await;
                    match ai.ext_ip_mapper.find_external_ip(&laddr.ip().to_string()) {
                        Ok(ip) => ip,
                        Err(err) => {
                            log::warn!(
                                "1:1 NAT mapping is enabled but no external IP is found for {}: {}",
                                laddr,
                                err
                            );
                            return Ok(());
                        }
                    }
                };

                let srflx_config = CandidateServerReflexiveConfig {
                    base_config: CandidateBaseConfig {
                        network: network.clone(),
                        address: mapped_ip.to_string(),
                        port: laddr.port(),
                        component: COMPONENT_RTP,
                        conn: Some(conn),
                        ..Default::default()
                    },
                    rel_addr: laddr.ip().to_string(),
                    rel_port: laddr.port(),
                };

                let candidate: Arc<dyn Candidate + Send + Sync> =
                    match srflx_config.new_candidate_server_reflexive().await {
                        Ok(candidate) => Arc::new(candidate),
                        Err(err) => {
                            log::warn!(
                                "Failed to create server reflexive candidate: {} {} {}: {}",
                                network,
                                mapped_ip,
                                laddr.port(),
                                err
                            );
                            return Ok(());
                        }
                    };

                {
                    let mut ai = agent_internal.lock().await;
                    if let Err(err) = ai.add_candidate(&candidate).await {
                        if let Err(close_err) = candidate.close().await {
                            log::warn!("Failed to close candidate: {}", close_err);
                        }
                        log::warn!(
                            "Failed to append to localCandidates and run onCandidateHdlr: {}",
                            err
                        );
                    }
                }

                Ok::<(), Error>(())
            });
        }

        wg.wait().await;
    }

    pub(crate) async fn gather_candidates_srflx(
        &self,
        urls: Vec<URL>,
        network_types: Vec<NetworkType>,
    ) {
        let (port_max, port_min) = (self.port_max, self.port_min);

        let wg = WaitGroup::new();
        for network_type in network_types {
            if network_type.is_tcp() {
                continue;
            }

            for url in &urls {
                let w = wg.worker();
                let network = network_type.to_string();
                let url = url.clone();
                let agent_internal = Arc::clone(&self.agent_internal);

                tokio::spawn(async move {
                    let _d = defer(move || {
                        drop(w);
                    });

                    let host_port = format!("{}:{}", url.host, url.port);
                    let server_addr = match SocketAddr::from_str(&host_port) {
                        Ok(addr) => addr,
                        Err(err) => {
                            log::warn!("failed to resolve stun host: {}: {}", host_port, err);
                            return Ok(());
                        }
                    };

                    let conn: Arc<dyn Conn + Send + Sync> = match listen_udp_in_port_range(
                        port_max,
                        port_min,
                        SocketAddr::from_str("0.0.0.0:0")?,
                    )
                    .await
                    {
                        Ok(conn) => Arc::new(conn),
                        Err(err) => {
                            log::warn!("Failed to listen for {}: {}", server_addr, err);
                            return Ok(());
                        }
                    };

                    let xoraddr =
                        match get_xormapped_addr(&conn, server_addr, STUN_GATHER_TIMEOUT).await {
                            Ok(xoraddr) => xoraddr,
                            Err(err) => {
                                log::warn!(
                                    "could not get server reflexive address {} {}: {}",
                                    network,
                                    url,
                                    err
                                );
                                return Ok(());
                            }
                        };

                    let (ip, port) = (xoraddr.ip, xoraddr.port);

                    let laddr = conn.local_addr()?;
                    let srflx_config = CandidateServerReflexiveConfig {
                        base_config: CandidateBaseConfig {
                            network: network.clone(),
                            address: ip.to_string(),
                            port,
                            component: COMPONENT_RTP,
                            conn: Some(conn),
                            ..Default::default()
                        },
                        rel_addr: laddr.ip().to_string(),
                        rel_port: laddr.port(),
                    };

                    let candidate: Arc<dyn Candidate + Send + Sync> =
                        match srflx_config.new_candidate_server_reflexive().await {
                            Ok(candidate) => Arc::new(candidate),
                            Err(err) => {
                                log::warn!(
                                    "Failed to create server reflexive candidate: {} {} {}: {}",
                                    network,
                                    ip,
                                    port,
                                    err
                                );
                                return Ok(());
                            }
                        };

                    {
                        let mut ai = agent_internal.lock().await;
                        if let Err(err) = ai.add_candidate(&candidate).await {
                            if let Err(close_err) = candidate.close().await {
                                log::warn!("Failed to close candidate: {}", close_err);
                            }
                            log::warn!(
                                "Failed to append to localCandidates and run onCandidateHdlr: {}",
                                err
                            );
                        }
                    }

                    Ok::<(), Error>(())
                });
            }
        }

        wg.wait().await;
    }

    pub(crate) async fn gather_candidates_relay(&self, urls: Vec<URL>) {
        let wg = WaitGroup::new();

        for url in urls {
            if url.scheme != SchemeType::TURN && url.scheme != SchemeType::TURNS {
                continue;
            } else if url.username.is_empty() {
                log::error!("Failed to gather relay candidates: {}", *ERR_USERNAME_EMPTY);
                return;
            } else if url.password.is_empty() {
                log::error!("Failed to gather relay candidates: {}", *ERR_PASSWORD_EMPTY);
                return;
            }

            let w = wg.worker();
            let network = NetworkType::UDP4.to_string();
            let agent_internal = Arc::clone(&self.agent_internal);

            tokio::spawn(async move {
                let _d = defer(move || {
                    drop(w);
                });

                let turn_server_addr = format!("{}:{}", url.host, url.port);

                let (loc_conn, rel_addr, rel_port) =
                    if url.proto == ProtoType::UDP && url.scheme == SchemeType::TURN {
                        let loc_conn = match UdpSocket::bind("0.0.0.0:0").await {
                            Ok(c) => c,
                            Err(err) => {
                                log::warn!("Failed to listen due to error: {}", err);
                                return Ok(());
                            }
                        };

                        let local_addr = loc_conn.local_addr()?;
                        let rel_addr = local_addr.ip().to_string();
                        let rel_port = local_addr.port();
                        (loc_conn, rel_addr, rel_port)
                    /*TODO:
                    } else if url.proto == ProtoType::UDP && url.scheme == SchemeType::TURNS{
                        udpAddr, connectErr := ResolveUDPAddr(network, turnserver_addr)
                        if connectErr != nil {
                            a.log.Warnf("Failed to resolve UDP Addr %s: %v\n", turnserver_addr, connectErr)
                            return
                        }

                        conn, connectErr := dtls.Dial(network, udpAddr, &dtls.Config{
                            InsecureSkipVerify: a.insecureSkipVerify, //nolint:gosec
                        })
                        if connectErr != nil {
                            a.log.Warnf("Failed to Dial DTLS Addr %s: %v\n", turnserver_addr, connectErr)
                            return
                        }

                        rel_addr = conn.LocalAddr().(*net.UDPAddr).IP.String()
                        rel_port = conn.LocalAddr().(*net.UDPAddr).Port
                        loc_conn = &fakePacketConn{conn}
                     */
                    //TODO: case a.proxyDialer != nil && url.Proto == ProtoTypeTCP && (url.Scheme == SchemeTypeTURN || url.Scheme == SchemeTypeTURNS):
                    //TODO: case url.Proto == ProtoTypeTCP && url.Scheme == SchemeTypeTURN:
                    //TODO: case url.Proto == ProtoTypeTCP && url.Scheme == SchemeTypeTURNS:
                    } else {
                        log::warn!("Unable to handle URL in gather_candidates_relay {}", url);
                        return Ok(());
                    };

                let cfg = turn::client::ClientConfig {
                    stun_serv_addr: String::new(),
                    turn_serv_addr: turn_server_addr.clone(),
                    username: url.username,
                    password: url.password,
                    realm: String::new(),
                    software: String::new(),
                    rto_in_ms: 0,
                    conn: Arc::new(loc_conn),
                };
                let client = match turn::client::Client::new(cfg).await {
                    Ok(client) => client,
                    Err(err) => {
                        log::warn!(
                            "Failed to build new turn.Client {} {}\n",
                            turn_server_addr,
                            err
                        );
                        return Ok(());
                    }
                };
                if let Err(err) = client.listen().await {
                    let _ = client.close().await;
                    log::warn!(
                        "Failed to listen on turn.Client {} {}",
                        turn_server_addr,
                        err
                    );
                    return Ok(());
                }

                let relay_conn = match client.allocate().await {
                    Ok(conn) => conn,
                    Err(err) => {
                        let _ = client.close().await;
                        log::warn!(
                            "Failed to allocate on turn.Client {} {}",
                            turn_server_addr,
                            err
                        );
                        return Ok(());
                    }
                };

                let raddr = relay_conn.local_addr()?;
                let relay_config = CandidateRelayConfig {
                    base_config: CandidateBaseConfig {
                        network: network.clone(),
                        address: raddr.ip().to_string(),
                        port: raddr.port(),
                        component: COMPONENT_RTP,
                        conn: Some(Arc::new(relay_conn)),
                        ..Default::default()
                    },
                    rel_addr,
                    rel_port,
                    //TODO: on_close: Option<OnClose>,
                    /*OnClose: func() error {
                        client.Close()
                        return locConn.Close()
                    },*/
                    ..Default::default()
                };

                let candidate: Arc<dyn Candidate + Send + Sync> =
                    match relay_config.new_candidate_relay().await {
                        Ok(candidate) => Arc::new(candidate),
                        Err(err) => {
                            let _ = client.close().await;
                            log::warn!(
                                "Failed to create relay candidate: {} {}: {}",
                                network,
                                raddr,
                                err
                            );
                            return Ok(());
                        }
                    };

                {
                    let mut ai = agent_internal.lock().await;
                    if let Err(err) = ai.add_candidate(&candidate).await {
                        if let Err(close_err) = candidate.close().await {
                            log::warn!("Failed to close candidate: {}", close_err);
                        }
                        log::warn!(
                            "Failed to append to localCandidates and run onCandidateHdlr: {}",
                            err
                        );
                    }
                }

                Ok::<(), Error>(())
            });
        }

        wg.wait().await;
    }
}
