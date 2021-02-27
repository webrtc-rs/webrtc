use super::*;
use crate::errors::*;
use crate::network_type::NetworkType;
use crate::url::{ProtoType, SchemeType, URL};
use crate::util::*;

use util::{Conn, Error};

use crate::candidate::candidate_base::CandidateBaseConfig;
use crate::candidate::candidate_relay::CandidateRelayConfig;
use crate::candidate::candidate_server_reflexive::CandidateServerReflexiveConfig;
use crate::candidate::*;
use defer::defer;
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use waitgroup::WaitGroup;

const STUN_GATHER_TIMEOUT: Duration = Duration::from_secs(5);

/*TODO:
func (a *Agent) gatherCandidates(ctx context.Context) {
    if err := a.setGatheringState(GatheringStateGathering); err != nil {
        a.log.Warnf("failed to set gatheringState to GatheringStateGathering: %v", err)
        return
    }

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

    if err := a.setGatheringState(GatheringStateComplete); err != nil {
        a.log.Warnf("failed to set gatheringState to GatheringStateComplete: %v", err)
    }
}


*/

impl Agent {
    pub(crate) async fn gather_candidates_local(&self, _network_types: Vec<NetworkType>) {
        /*

        localIPs, err := localInterfaces(a.net, a.interfaceFilter, network_types)
        if err != nil {
            a.log.Warnf("failed to iterate local interfaces, host candidates will not be gathered %s", err)
            return
        }

        for _, ip := range localIPs {
            mappedIP := ip
            if a.mDNSMode != MulticastDNSModeQueryAndGather && a.extIPMapper != nil && a.extIPMapper.candidateType == CandidateTypeHost {
                if _mappedIP, err := a.extIPMapper.findExternalIP(ip.String()); err == nil {
                    mappedIP = _mappedIP
                } else {
                    a.log.Warnf("1:1 NAT mapping is enabled but no external IP is found for %s\n", ip.String())
                }
            }

            address := mappedIP.String()
            if a.mDNSMode == MulticastDNSModeQueryAndGather {
                address = a.mDNSName
            }

            for network := range networks {
                var port int
                var conn net.PacketConn
                var err error

                var tcpType TCPType
                switch network {
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
                case udp:
                    conn, err = listen_udpin_port_range(a.net, a.log, int(a.portmax), int(a.portmin), network, &net.UDPAddr{IP: ip, Port: 0})
                    if err != nil {
                        a.log.Warnf("could not listen %s %s\n", network, ip)
                        continue
                    }

                    port = conn.LocalAddr().(*net.UDPAddr).Port
                }
                hostConfig := CandidateHostConfig{
                    Network:   network,
                    Address:   address,
                    Port:      port,
                    Component: ComponentRTP,
                    TCPType:   tcpType,
                }

                c, err := NewCandidateHost(&hostConfig)
                if err != nil {
                    closeConnAndLog(conn, a.log, fmt.Sprintf("Failed to create host candidate: %s %s %d: %v\n", network, mappedIP, port, err))
                    continue
                }

                if a.mDNSMode == MulticastDNSModeQueryAndGather {
                    if err = c.setIP(ip); err != nil {
                        closeConnAndLog(conn, a.log, fmt.Sprintf("Failed to create host candidate: %s %s %d: %v\n", network, mappedIP, port, err))
                        continue
                    }
                }

                if err := a.addCandidate(ctx, c, conn); err != nil {
                    if closeErr := c.close(); closeErr != nil {
                        a.log.Warnf("Failed to close candidate: %v", closeErr)
                    }
                    a.log.Warnf("Failed to append to localCandidates and run onCandidateHdlr: %v\n", err)
                }
            }
        }*/
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
