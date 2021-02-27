use super::*;
use crate::errors::*;
use crate::network_type::NetworkType;
use crate::url::{ProtoType, SchemeType, URL};

use util::{Conn, Error};

use crate::candidate::candidate_base::CandidateBaseConfig;
use crate::candidate::candidate_relay::CandidateRelayConfig;
use crate::candidate::*;
use defer::defer;
use std::sync::Arc;
use tokio::net::UdpSocket;
use waitgroup::WaitGroup;

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
                a.gatherCandidatesLocal(ctx, a.networkTypes)
                wg.Done()
            }()
        case CandidateTypeServerReflexive:
            wg.Add(1)
            go func() {
                a.gatherCandidatesSrflx(ctx, a.urls, a.networkTypes)
                wg.Done()
            }()
            if a.extIPMapper != nil && a.extIPMapper.candidateType == CandidateTypeServerReflexive {
                wg.Add(1)
                go func() {
                    a.gatherCandidatesSrflxMapped(ctx, a.networkTypes)
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

func (a *Agent) gatherCandidatesLocal(ctx context.Context, networkTypes []NetworkType) { //nolint:gocognit
    networks := map[string]struct{}{}
    for _, networkType := range networkTypes {
        if networkType.IsTCP() {
            networks[tcp] = struct{}{}
        } else {
            networks[udp] = struct{}{}
        }
    }

    localIPs, err := localInterfaces(a.net, a.interfaceFilter, networkTypes)
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
                conn, err = listenUDPInPortRange(a.net, a.log, int(a.portmax), int(a.portmin), network, &net.UDPAddr{IP: ip, Port: 0})
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
    }
}

func (a *Agent) gatherCandidatesSrflxMapped(ctx context.Context, networkTypes []NetworkType) {
    var wg sync.WaitGroup
    defer wg.Wait()

    for _, networkType := range networkTypes {
        if networkType.IsTCP() {
            continue
        }

        network := networkType.String()
        wg.Add(1)
        go func() {
            defer wg.Done()
            conn, err := listenUDPInPortRange(a.net, a.log, int(a.portmax), int(a.portmin), network, &net.UDPAddr{IP: nil, Port: 0})
            if err != nil {
                a.log.Warnf("Failed to listen %s: %v\n", network, err)
                return
            }

            laddr := conn.LocalAddr().(*net.UDPAddr)
            mappedIP, err := a.extIPMapper.findExternalIP(laddr.IP.String())
            if err != nil {
                closeConnAndLog(conn, a.log, fmt.Sprintf("1:1 NAT mapping is enabled but no external IP is found for %s\n", laddr.IP.String()))
                return
            }

            srflxConfig := CandidateServerReflexiveConfig{
                Network:   network,
                Address:   mappedIP.String(),
                Port:      laddr.Port,
                Component: ComponentRTP,
                RelAddr:   laddr.IP.String(),
                RelPort:   laddr.Port,
            }
            c, err := NewCandidateServerReflexive(&srflxConfig)
            if err != nil {
                closeConnAndLog(conn, a.log, fmt.Sprintf("Failed to create server reflexive candidate: %s %s %d: %v\n",
                    network,
                    mappedIP.String(),
                    laddr.Port,
                    err))
                return
            }

            if err := a.addCandidate(ctx, c, conn); err != nil {
                if closeErr := c.close(); closeErr != nil {
                    a.log.Warnf("Failed to close candidate: %v", closeErr)
                }
                a.log.Warnf("Failed to append to localCandidates and run onCandidateHdlr: %v\n", err)
            }
        }()
    }
}

func (a *Agent) gatherCandidatesSrflx(ctx context.Context, urls []*URL, networkTypes []NetworkType) {
    var wg sync.WaitGroup
    defer wg.Wait()

    for _, networkType := range networkTypes {
        if networkType.IsTCP() {
            continue
        }

        for i := range urls {
            wg.Add(1)
            go func(url URL, network string) {
                defer wg.Done()
                hostPort := fmt.Sprintf("%s:%d", url.Host, url.Port)
                serverAddr, err := a.net.ResolveUDPAddr(network, hostPort)
                if err != nil {
                    a.log.Warnf("failed to resolve stun host: %s: %v", hostPort, err)
                    return
                }

                conn, err := listenUDPInPortRange(a.net, a.log, int(a.portmax), int(a.portmin), network, &net.UDPAddr{IP: nil, Port: 0})
                if err != nil {
                    closeConnAndLog(conn, a.log, fmt.Sprintf("Failed to listen for %s: %v\n", serverAddr.String(), err))
                    return
                }

                xoraddr, err := getXORMappedAddr(conn, serverAddr, stunGatherTimeout)
                if err != nil {
                    closeConnAndLog(conn, a.log, fmt.Sprintf("could not get server reflexive address %s %s: %v\n", network, url, err))
                    return
                }

                ip := xoraddr.IP
                port := xoraddr.Port

                laddr := conn.LocalAddr().(*net.UDPAddr)
                srflxConfig := CandidateServerReflexiveConfig{
                    Network:   network,
                    Address:   ip.String(),
                    Port:      port,
                    Component: ComponentRTP,
                    RelAddr:   laddr.IP.String(),
                    RelPort:   laddr.Port,
                }
                c, err := NewCandidateServerReflexive(&srflxConfig)
                if err != nil {
                    closeConnAndLog(conn, a.log, fmt.Sprintf("Failed to create server reflexive candidate: %s %s %d: %v\n", network, ip, port, err))
                    return
                }

                if err := a.addCandidate(ctx, c, conn); err != nil {
                    if closeErr := c.close(); closeErr != nil {
                        a.log.Warnf("Failed to close candidate: %v", closeErr)
                    }
                    a.log.Warnf("Failed to append to localCandidates and run onCandidateHdlr: %v\n", err)
                }
            }(*urls[i], networkType.String())
        }
    }
}

*/

impl Agent {
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

            let network = NetworkType::UDP4.to_string();
            let w = wg.worker();
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
                        udpAddr, connectErr := util::conn::lookup_host()ResolveUDPAddr(network, turnserver_addr)
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
