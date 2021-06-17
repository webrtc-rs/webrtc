use super::*;
use crate::errors::*;
use crate::network_type::*;
use crate::url::{ProtoType, SchemeType, Url};
use crate::util::*;

use util::{vnet::net::*, Conn, Error};

use crate::candidate::candidate_base::CandidateBaseConfig;
use crate::candidate::candidate_host::CandidateHostConfig;
use crate::candidate::candidate_relay::CandidateRelayConfig;
use crate::candidate::candidate_server_reflexive::CandidateServerReflexiveConfig;
use crate::candidate::*;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use std::sync::Arc;
use waitgroup::WaitGroup;

const STUN_GATHER_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct GatherCandidatesInternalParams {
    pub(crate) candidate_types: Vec<CandidateType>,
    pub(crate) urls: Vec<Url>,
    pub(crate) network_types: Vec<NetworkType>,
    pub(crate) port_max: u16,
    pub(crate) port_min: u16,
    pub(crate) mdns_mode: MulticastDnsMode,
    pub(crate) mdns_name: String,
    pub(crate) net: Arc<Net>,
    pub(crate) interface_filter: Arc<Option<InterfaceFilterFn>>,
    pub(crate) ext_ip_mapper: Arc<Option<ExternalIpMapper>>,
    pub(crate) agent_internal: Arc<Mutex<AgentInternal>>,
    pub(crate) gathering_state: Arc<AtomicU8>,
    pub(crate) chan_candidate_tx: ChanCandidateTx,
}

struct GatherCandidatesLocalParams {
    network_types: Vec<NetworkType>,
    port_max: u16,
    port_min: u16,
    mdns_mode: MulticastDnsMode,
    mdns_name: String,
    interface_filter: Arc<Option<InterfaceFilterFn>>,
    ext_ip_mapper: Arc<Option<ExternalIpMapper>>,
    net: Arc<Net>,
    agent_internal: Arc<Mutex<AgentInternal>>,
}

struct GatherCandidatesSrflxMappedParasm {
    network_types: Vec<NetworkType>,
    port_max: u16,
    port_min: u16,
    ext_ip_mapper: Arc<Option<ExternalIpMapper>>,
    net: Arc<Net>,
    agent_internal: Arc<Mutex<AgentInternal>>,
}

struct GatherCandidatesSrflxParams {
    urls: Vec<Url>,
    network_types: Vec<NetworkType>,
    port_max: u16,
    port_min: u16,
    net: Arc<Net>,
    agent_internal: Arc<Mutex<AgentInternal>>,
}

impl Agent {
    pub(crate) async fn gather_candidates_internal(params: GatherCandidatesInternalParams) {
        Self::set_gathering_state(
            &params.chan_candidate_tx,
            &params.gathering_state,
            GatheringState::Gathering,
        )
        .await;

        let wg = WaitGroup::new();

        for t in &params.candidate_types {
            match t {
                CandidateType::Host => {
                    let local_params = GatherCandidatesLocalParams {
                        network_types: params.network_types.clone(),
                        port_max: params.port_max,
                        port_min: params.port_min,
                        mdns_mode: params.mdns_mode,
                        mdns_name: params.mdns_name.clone(),
                        interface_filter: Arc::clone(&params.interface_filter),
                        ext_ip_mapper: Arc::clone(&params.ext_ip_mapper),
                        net: Arc::clone(&params.net),
                        agent_internal: Arc::clone(&params.agent_internal),
                    };

                    let w = wg.worker();
                    tokio::spawn(async move {
                        let _d = w;

                        Self::gather_candidates_local(local_params).await;
                    });
                }
                CandidateType::ServerReflexive => {
                    let srflx_params = GatherCandidatesSrflxParams {
                        urls: params.urls.clone(),
                        network_types: params.network_types.clone(),
                        port_max: params.port_max,
                        port_min: params.port_min,
                        net: Arc::clone(&params.net),
                        agent_internal: Arc::clone(&params.agent_internal),
                    };
                    let w1 = wg.worker();
                    tokio::spawn(async move {
                        let _d = w1;

                        Self::gather_candidates_srflx(srflx_params).await;
                    });
                    if let Some(ext_ip_mapper) = &*params.ext_ip_mapper {
                        if ext_ip_mapper.candidate_type == CandidateType::ServerReflexive {
                            let srflx_mapped_params = GatherCandidatesSrflxMappedParasm {
                                network_types: params.network_types.clone(),
                                port_max: params.port_max,
                                port_min: params.port_min,
                                ext_ip_mapper: Arc::clone(&params.ext_ip_mapper),
                                net: Arc::clone(&params.net),
                                agent_internal: Arc::clone(&params.agent_internal),
                            };
                            let w2 = wg.worker();
                            tokio::spawn(async move {
                                let _d = w2;

                                Self::gather_candidates_srflx_mapped(srflx_mapped_params).await;
                            });
                        }
                    }
                }
                CandidateType::Relay => {
                    let urls = params.urls.clone();
                    let net = Arc::clone(&params.net);
                    let agent_internal = Arc::clone(&params.agent_internal);
                    let w = wg.worker();
                    tokio::spawn(async move {
                        let _d = w;

                        Self::gather_candidates_relay(urls, net, agent_internal).await;
                    });
                }
                _ => {}
            }
        }

        // Block until all STUN and TURN URLs have been gathered (or timed out)
        wg.wait().await;

        Self::set_gathering_state(
            &params.chan_candidate_tx,
            &params.gathering_state,
            GatheringState::Complete,
        )
        .await;
    }

    async fn set_gathering_state(
        chan_candidate_tx: &ChanCandidateTx,
        gathering_state: &Arc<AtomicU8>,
        new_state: GatheringState,
    ) {
        if GatheringState::from(gathering_state.load(Ordering::SeqCst)) != new_state
            && new_state == GatheringState::Complete
        {
            if let Some(tx) = chan_candidate_tx {
                let _ = tx.send(None).await;
            }
        }

        gathering_state.store(new_state as u8, Ordering::SeqCst);
    }

    async fn gather_candidates_local(params: GatherCandidatesLocalParams) {
        let (
            network_types,
            port_max,
            port_min,
            mdns_mode,
            mdns_name,
            interface_filter,
            ext_ip_mapper,
            net,
            agent_internal,
        ) = (
            params.network_types,
            params.port_max,
            params.port_min,
            params.mdns_mode,
            params.mdns_name,
            params.interface_filter,
            params.ext_ip_mapper,
            params.net,
            params.agent_internal,
        );

        let ips = local_interfaces(&net, &*interface_filter, &network_types).await;
        for ip in ips {
            let mut mapped_ip = ip;

            if mdns_mode != MulticastDnsMode::QueryAndGather && ext_ip_mapper.is_some() {
                if let Some(ext_ip_mapper2) = &*ext_ip_mapper {
                    if ext_ip_mapper2.candidate_type == CandidateType::Host {
                        if let Ok(mi) = ext_ip_mapper2.find_external_ip(&ip.to_string()) {
                            mapped_ip = mi;
                        } else {
                            log::warn!(
                                "1:1 NAT mapping is enabled but no external IP is found for {}",
                                ip
                            );
                        }
                    }
                }
            }

            let address = if mdns_mode == MulticastDnsMode::QueryAndGather {
                mdns_name.clone()
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
                    &net,
                    port_max,
                    port_min,
                    SocketAddr::new(ip, 0),
                )
                .await
                {
                    Ok(conn) => conn,
                    Err(err) => {
                        log::warn!("could not listen {} {}: {}", network, ip, err);
                        continue;
                    }
                };

                let port = match conn.local_addr().await {
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
                        ..CandidateBaseConfig::default()
                    },
                    ..CandidateHostConfig::default()
                };

                let candidate: Arc<dyn Candidate + Send + Sync> = match host_config
                    .new_candidate_host(Some(agent_internal.clone()))
                    .await
                {
                    Ok(candidate) => {
                        if mdns_mode == MulticastDnsMode::QueryAndGather {
                            if let Err(err) = candidate.set_ip(&ip).await {
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
            }
        }
    }

    async fn gather_candidates_srflx_mapped(params: GatherCandidatesSrflxMappedParasm) {
        let (network_types, port_max, port_min, ext_ip_mapper, net, agent_internal) = (
            params.network_types,
            params.port_max,
            params.port_min,
            params.ext_ip_mapper,
            params.net,
            params.agent_internal,
        );

        let wg = WaitGroup::new();

        for network_type in network_types {
            if network_type.is_tcp() {
                continue;
            }

            let network = network_type.to_string();
            let net2 = Arc::clone(&net);
            let agent_internal2 = Arc::clone(&agent_internal);
            let ext_ip_mapper2 = Arc::clone(&ext_ip_mapper);

            let w = wg.worker();
            tokio::spawn(async move {
                let _d = w;

                let conn: Arc<dyn Conn + Send + Sync> = match listen_udp_in_port_range(
                    &net2,
                    port_max,
                    port_min,
                    if network_type.is_ipv4() {
                        SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0)
                    } else {
                        SocketAddr::new(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0).into(), 0)
                    },
                )
                .await
                {
                    Ok(conn) => conn,
                    Err(err) => {
                        log::warn!("Failed to listen {}: {}", network, err);
                        return Ok(());
                    }
                };

                let laddr = conn.local_addr().await?;
                let mapped_ip = {
                    if let Some(ext_ip_mapper3) = &*ext_ip_mapper2 {
                        match ext_ip_mapper3.find_external_ip(&laddr.ip().to_string()) {
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
                    } else {
                        log::error!("ext_ip_mapper is None in gather_candidates_srflx_mapped");
                        return Ok(());
                    }
                };

                let srflx_config = CandidateServerReflexiveConfig {
                    base_config: CandidateBaseConfig {
                        network: network.clone(),
                        address: mapped_ip.to_string(),
                        port: laddr.port(),
                        component: COMPONENT_RTP,
                        conn: Some(conn),
                        ..CandidateBaseConfig::default()
                    },
                    rel_addr: laddr.ip().to_string(),
                    rel_port: laddr.port(),
                };

                let candidate: Arc<dyn Candidate + Send + Sync> = match srflx_config
                    .new_candidate_server_reflexive(Some(agent_internal2.clone()))
                    .await
                {
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
                    let mut ai = agent_internal2.lock().await;
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

    async fn gather_candidates_srflx(params: GatherCandidatesSrflxParams) {
        let (urls, network_types, port_max, port_min, net, agent_internal) = (
            params.urls,
            params.network_types,
            params.port_max,
            params.port_min,
            params.net,
            params.agent_internal,
        );

        let wg = WaitGroup::new();
        for network_type in network_types {
            if network_type.is_tcp() {
                continue;
            }

            for url in &urls {
                let network = network_type.to_string();
                let is_ipv4 = network_type.is_ipv4();
                let url = url.clone();
                let net2 = Arc::clone(&net);
                let agent_internal2 = Arc::clone(&agent_internal);

                let w = wg.worker();
                tokio::spawn(async move {
                    let _d = w;

                    let host_port = format!("{}:{}", url.host, url.port);
                    let server_addr = match net2.resolve_addr(is_ipv4, &host_port).await {
                        Ok(addr) => addr,
                        Err(err) => {
                            log::warn!("failed to resolve stun host: {}: {}", host_port, err);
                            return Ok(());
                        }
                    };

                    let conn: Arc<dyn Conn + Send + Sync> = match listen_udp_in_port_range(
                        &net2,
                        port_max,
                        port_min,
                        if is_ipv4 {
                            SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0)
                        } else {
                            SocketAddr::new(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0).into(), 0)
                        },
                    )
                    .await
                    {
                        Ok(conn) => conn,
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

                    let laddr = conn.local_addr().await?;
                    let srflx_config = CandidateServerReflexiveConfig {
                        base_config: CandidateBaseConfig {
                            network: network.clone(),
                            address: ip.to_string(),
                            port,
                            component: COMPONENT_RTP,
                            conn: Some(conn),
                            ..CandidateBaseConfig::default()
                        },
                        rel_addr: laddr.ip().to_string(),
                        rel_port: laddr.port(),
                    };

                    let candidate: Arc<dyn Candidate + Send + Sync> = match srflx_config
                        .new_candidate_server_reflexive(Some(agent_internal2.clone()))
                        .await
                    {
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
                        let mut ai = agent_internal2.lock().await;
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

    pub(crate) async fn gather_candidates_relay(
        urls: Vec<Url>,
        net: Arc<Net>,
        agent_internal: Arc<Mutex<AgentInternal>>,
    ) {
        let wg = WaitGroup::new();

        for url in urls {
            if url.scheme != SchemeType::Turn && url.scheme != SchemeType::Turns {
                continue;
            }
            if url.username.is_empty() {
                log::error!("Failed to gather relay candidates: {}", *ERR_USERNAME_EMPTY);
                return;
            }
            if url.password.is_empty() {
                log::error!("Failed to gather relay candidates: {}", *ERR_PASSWORD_EMPTY);
                return;
            }

            let network = NetworkType::Udp4.to_string();
            let net2 = Arc::clone(&net);
            let agent_internal2 = Arc::clone(&agent_internal);

            let w = wg.worker();
            tokio::spawn(async move {
                let _d = w;

                let turn_server_addr = format!("{}:{}", url.host, url.port);

                let (loc_conn, rel_addr, rel_port) =
                    if url.proto == ProtoType::Udp && url.scheme == SchemeType::Turn {
                        let loc_conn = match net2.bind(SocketAddr::from_str("0.0.0.0:0")?).await {
                            Ok(c) => c,
                            Err(err) => {
                                log::warn!("Failed to listen due to error: {}", err);
                                return Ok(());
                            }
                        };

                        let local_addr = loc_conn.local_addr().await?;
                        let rel_addr = local_addr.ip().to_string();
                        let rel_port = local_addr.port();
                        (loc_conn, rel_addr, rel_port)
                    /*TODO: case url.proto == ProtoType::UDP && url.scheme == SchemeType::TURNS{
                    case a.proxyDialer != nil && url.Proto == ProtoTypeTCP && (url.Scheme == SchemeTypeTURN || url.Scheme == SchemeTypeTURNS):
                    case url.Proto == ProtoTypeTCP && url.Scheme == SchemeTypeTURN:
                    case url.Proto == ProtoTypeTCP && url.Scheme == SchemeTypeTURNS:*/
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
                    conn: loc_conn,
                    vnet: Some(Arc::clone(&net2)),
                };
                let client = match turn::client::Client::new(cfg).await {
                    Ok(client) => Arc::new(client),
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

                let raddr = relay_conn.local_addr().await?;
                let relay_config = CandidateRelayConfig {
                    base_config: CandidateBaseConfig {
                        network: network.clone(),
                        address: raddr.ip().to_string(),
                        port: raddr.port(),
                        component: COMPONENT_RTP,
                        conn: Some(Arc::new(relay_conn)),
                        ..CandidateBaseConfig::default()
                    },
                    rel_addr,
                    rel_port,
                    relay_client: Some(Arc::clone(&client)),
                };

                let candidate: Arc<dyn Candidate + Send + Sync> = match relay_config
                    .new_candidate_relay(Some(agent_internal2.clone()))
                    .await
                {
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
                    let mut ai = agent_internal2.lock().await;
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
