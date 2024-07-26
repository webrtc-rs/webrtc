use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use ice::candidate::Candidate;
use ice::state::ConnectionState;
use ice_candidate::RTCIceCandidate;
use ice_candidate_pair::RTCIceCandidatePair;
use ice_gatherer::RTCIceGatherer;
use ice_role::RTCIceRole;
use portable_atomic::AtomicU8;
use tokio::sync::{mpsc, Mutex};
use util::Conn;

use crate::error::{flatten_errs, Error, Result};
use crate::ice_transport::ice_parameters::RTCIceParameters;
use crate::ice_transport::ice_transport_state::RTCIceTransportState;
use crate::mux::endpoint::Endpoint;
use crate::mux::mux_func::MatchFunc;
use crate::mux::{Config, Mux};
use crate::stats::stats_collector::StatsCollector;
use crate::stats::ICETransportStats;
use crate::stats::StatsReportType::Transport;

#[cfg(test)]
mod ice_transport_test;

pub mod ice_candidate;
pub mod ice_candidate_pair;
pub mod ice_candidate_type;
pub mod ice_connection_state;
pub mod ice_gatherer;
pub mod ice_gatherer_state;
pub mod ice_gathering_state;
pub mod ice_parameters;
pub mod ice_protocol;
pub mod ice_role;
pub mod ice_server;
pub mod ice_transport_state;

pub type OnConnectionStateChangeHdlrFn = Box<
    dyn (FnMut(RTCIceTransportState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnSelectedCandidatePairChangeHdlrFn = Box<
    dyn (FnMut(RTCIceCandidatePair) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

#[derive(Default)]
struct ICETransportInternal {
    role: RTCIceRole,
    conn: Option<Arc<dyn Conn + Send + Sync>>, //AgentConn
    mux: Option<Mux>,
    cancel_tx: Option<mpsc::Sender<()>>,
}

/// ICETransport allows an application access to information about the ICE
/// transport over which packets are sent and received.
#[derive(Default)]
pub struct RTCIceTransport {
    pub(crate) gatherer: Arc<RTCIceGatherer>,
    on_connection_state_change_handler: Arc<ArcSwapOption<Mutex<OnConnectionStateChangeHdlrFn>>>,
    on_selected_candidate_pair_change_handler:
        Arc<ArcSwapOption<Mutex<OnSelectedCandidatePairChangeHdlrFn>>>,
    state: Arc<AtomicU8>, // ICETransportState
    internal: Mutex<ICETransportInternal>,
}

impl RTCIceTransport {
    /// creates a new new_icetransport.
    pub(crate) fn new(gatherer: Arc<RTCIceGatherer>) -> Self {
        RTCIceTransport {
            state: Arc::new(AtomicU8::new(RTCIceTransportState::New as u8)),
            gatherer,
            ..Default::default()
        }
    }

    /// get_selected_candidate_pair returns the selected candidate pair on which packets are sent
    /// if there is no selected pair nil is returned
    pub async fn get_selected_candidate_pair(&self) -> Option<RTCIceCandidatePair> {
        if let Some(agent) = self.gatherer.get_agent().await {
            if let Some(ice_pair) = agent.get_selected_candidate_pair() {
                let local = RTCIceCandidate::from(&ice_pair.local);
                let remote = RTCIceCandidate::from(&ice_pair.remote);
                return Some(RTCIceCandidatePair::new(local, remote));
            }
        }
        None
    }

    /// Start incoming connectivity checks based on its configured role.
    pub async fn start(&self, params: &RTCIceParameters, role: Option<RTCIceRole>) -> Result<()> {
        if self.state() != RTCIceTransportState::New {
            return Err(Error::ErrICETransportNotInNew);
        }

        self.ensure_gatherer().await?;

        if let Some(agent) = self.gatherer.get_agent().await {
            let state = Arc::clone(&self.state);

            let on_connection_state_change_handler =
                Arc::clone(&self.on_connection_state_change_handler);
            agent.on_connection_state_change(Box::new(move |ice_state: ConnectionState| {
                let s = RTCIceTransportState::from(ice_state);
                let on_connection_state_change_handler_clone =
                    Arc::clone(&on_connection_state_change_handler);
                state.store(s as u8, Ordering::SeqCst);
                Box::pin(async move {
                    if let Some(handler) = &*on_connection_state_change_handler_clone.load() {
                        let mut f = handler.lock().await;
                        f(s).await;
                    }
                })
            }));

            let on_selected_candidate_pair_change_handler =
                Arc::clone(&self.on_selected_candidate_pair_change_handler);
            agent.on_selected_candidate_pair_change(Box::new(
                move |local: &Arc<dyn Candidate + Send + Sync>,
                      remote: &Arc<dyn Candidate + Send + Sync>| {
                    let on_selected_candidate_pair_change_handler_clone =
                        Arc::clone(&on_selected_candidate_pair_change_handler);
                    let local = RTCIceCandidate::from(local);
                    let remote = RTCIceCandidate::from(remote);
                    Box::pin(async move {
                        if let Some(handler) =
                            &*on_selected_candidate_pair_change_handler_clone.load()
                        {
                            let mut f = handler.lock().await;
                            f(RTCIceCandidatePair::new(local, remote)).await;
                        }
                    })
                },
            ));

            let role = if let Some(role) = role {
                role
            } else {
                RTCIceRole::Controlled
            };

            let (cancel_tx, cancel_rx) = mpsc::channel(1);
            {
                let mut internal = self.internal.lock().await;
                internal.role = role;
                internal.cancel_tx = Some(cancel_tx);
            }

            let conn: Arc<dyn Conn + Send + Sync> = match role {
                RTCIceRole::Controlling => {
                    agent
                        .dial(
                            cancel_rx,
                            params.username_fragment.clone(),
                            params.password.clone(),
                        )
                        .await?
                }

                RTCIceRole::Controlled => {
                    agent
                        .accept(
                            cancel_rx,
                            params.username_fragment.clone(),
                            params.password.clone(),
                        )
                        .await?
                }

                _ => return Err(Error::ErrICERoleUnknown),
            };

            let config = Config {
                conn: Arc::clone(&conn),
                buffer_size: self.gatherer.setting_engine.get_receive_mtu(),
            };

            {
                let mut internal = self.internal.lock().await;
                internal.conn = Some(conn);
                internal.mux = Some(Mux::new(config));
            }

            Ok(())
        } else {
            Err(Error::ErrICEAgentNotExist)
        }
    }

    /// restart is not exposed currently because ORTC has users create a whole new ICETransport
    /// so for now lets keep it private so we don't cause ORTC users to depend on non-standard APIs
    pub(crate) async fn restart(&self) -> Result<()> {
        if let Some(agent) = self.gatherer.get_agent().await {
            agent
                .restart(
                    self.gatherer
                        .setting_engine
                        .candidates
                        .username_fragment
                        .clone(),
                    self.gatherer.setting_engine.candidates.password.clone(),
                )
                .await?;
        } else {
            return Err(Error::ErrICEAgentNotExist);
        }
        self.gatherer.gather().await
    }

    /// Stop irreversibly stops the ICETransport.
    pub async fn stop(&self) -> Result<()> {
        self.set_state(RTCIceTransportState::Closed);

        let mut errs: Vec<Error> = vec![];
        {
            let mut internal = self.internal.lock().await;
            internal.cancel_tx.take();
            if let Some(mut mux) = internal.mux.take() {
                mux.close().await;
            }
            if let Some(conn) = internal.conn.take() {
                if let Err(err) = conn.close().await {
                    errs.push(err.into());
                }
            }
        }

        if let Err(err) = self.gatherer.close().await {
            errs.push(err);
        }

        flatten_errs(errs)
    }

    /// on_selected_candidate_pair_change sets a handler that is invoked when a new
    /// ICE candidate pair is selected
    pub fn on_selected_candidate_pair_change(&self, f: OnSelectedCandidatePairChangeHdlrFn) {
        self.on_selected_candidate_pair_change_handler
            .store(Some(Arc::new(Mutex::new(f))));
    }

    /// on_connection_state_change sets a handler that is fired when the ICE
    /// connection state changes.
    pub fn on_connection_state_change(&self, f: OnConnectionStateChangeHdlrFn) {
        self.on_connection_state_change_handler
            .store(Some(Arc::new(Mutex::new(f))));
    }

    /// Role indicates the current role of the ICE transport.
    pub async fn role(&self) -> RTCIceRole {
        let internal = self.internal.lock().await;
        internal.role
    }

    /// set_remote_candidates sets the sequence of candidates associated with the remote ICETransport.
    pub async fn set_remote_candidates(&self, remote_candidates: &[RTCIceCandidate]) -> Result<()> {
        self.ensure_gatherer().await?;

        if let Some(agent) = self.gatherer.get_agent().await {
            for rc in remote_candidates {
                let c: Arc<dyn Candidate + Send + Sync> = Arc::new(rc.to_ice()?);
                agent.add_remote_candidate(&c)?;
            }
            Ok(())
        } else {
            Err(Error::ErrICEAgentNotExist)
        }
    }

    /// adds a candidate associated with the remote ICETransport.
    pub async fn add_remote_candidate(
        &self,
        remote_candidate: Option<RTCIceCandidate>,
    ) -> Result<()> {
        self.ensure_gatherer().await?;

        if let Some(agent) = self.gatherer.get_agent().await {
            if let Some(r) = remote_candidate {
                let c: Arc<dyn Candidate + Send + Sync> = Arc::new(r.to_ice()?);
                agent.add_remote_candidate(&c)?;
            }

            Ok(())
        } else {
            Err(Error::ErrICEAgentNotExist)
        }
    }

    /// State returns the current ice transport state.
    pub fn state(&self) -> RTCIceTransportState {
        RTCIceTransportState::from(self.state.load(Ordering::SeqCst))
    }

    pub(crate) fn set_state(&self, s: RTCIceTransportState) {
        self.state.store(s as u8, Ordering::SeqCst)
    }

    pub(crate) async fn new_endpoint(&self, f: MatchFunc) -> Option<Arc<Endpoint>> {
        let internal = self.internal.lock().await;
        if let Some(mux) = &internal.mux {
            Some(mux.new_endpoint(f).await)
        } else {
            None
        }
    }

    pub(crate) async fn ensure_gatherer(&self) -> Result<()> {
        if self.gatherer.get_agent().await.is_none() {
            self.gatherer.create_agent().await
        } else {
            Ok(())
        }
    }

    pub(crate) async fn collect_stats(&self, collector: &StatsCollector) {
        if let Some(agent) = self.gatherer.get_agent().await {
            let stats = ICETransportStats::new("ice_transport".to_string(), agent);

            collector.insert("ice_transport".to_string(), Transport(stats));
        }
    }

    pub(crate) async fn have_remote_credentials_change(
        &self,
        new_ufrag: &str,
        new_pwd: &str,
    ) -> bool {
        if let Some(agent) = self.gatherer.get_agent().await {
            let (ufrag, upwd) = agent.get_remote_user_credentials().await;
            ufrag != new_ufrag || upwd != new_pwd
        } else {
            false
        }
    }

    pub(crate) async fn set_remote_credentials(
        &self,
        new_ufrag: String,
        new_pwd: String,
    ) -> Result<()> {
        if let Some(agent) = self.gatherer.get_agent().await {
            Ok(agent.set_remote_credentials(new_ufrag, new_pwd).await?)
        } else {
            Err(Error::ErrICEAgentNotExist)
        }
    }
}
