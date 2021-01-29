use super::utils::*;
use super::*;
use crate::allocation::five_tuple::*;
use crate::errors::*;
use crate::proto::*;

use crate::allocation::channel_bind::ChannelBind;
use crate::allocation::permission::Permission;
use crate::proto::channum::ChannelNumber;
use crate::proto::data::Data;
use crate::proto::evenport::EvenPort;
use crate::proto::lifetime::*;
use crate::proto::peeraddr::PeerAddress;
use crate::proto::relayaddr::RelayedAddress;
use crate::proto::reqtrans::RequestedTransport;
use crate::proto::rsrvtoken::ReservationToken;

use stun::attributes::*;
use stun::error_code::*;
use stun::fingerprint::*;
use stun::uattrs::*;
use stun::xoraddr::*;

impl Request {
    pub(crate) async fn handle_binding_request(&mut self, m: &Message) -> Result<(), Error> {
        log::debug!("received BindingRequest from {}", self.src_addr);

        let (ip, port) = (self.src_addr.ip(), self.src_addr.port());

        let attrs = build_msg(
            m.transaction_id,
            BINDING_SUCCESS,
            vec![
                Box::new(XORMappedAddress { ip, port }),
                Box::new(FINGERPRINT),
            ],
        );

        build_and_send(&self.conn, self.src_addr, &attrs).await
    }

    // // https://tools.ietf.org/html/rfc5766#section-6.2
    pub(crate) async fn handle_allocate_request(&mut self, m: &Message) -> Result<(), Error> {
        log::debug!("received AllocateRequest from {}", self.src_addr);

        // 1. The server MUST require that the request be authenticated.  This
        //    authentication MUST be done using the long-term credential
        //    mechanism of [https://tools.ietf.org/html/rfc5389#section-10.2.2]
        //    unless the client and server agree to use another mechanism through
        //    some procedure outside the scope of this document.
        let message_integrity = self.authenticate_request(m, METHOD_ALLOCATE).await?;
        let five_tuple = FiveTuple {
            src_addr: self.src_addr,
            dst_addr: self.conn.local_addr()?,
            protocol: PROTO_UDP,
        };
        let mut requested_port = 0;
        let mut reservation_token = "".to_owned();

        let bad_request_msg = build_msg(
            m.transaction_id,
            MessageType::new(METHOD_ALLOCATE, CLASS_ERROR_RESPONSE),
            vec![Box::new(ErrorCodeAttribute {
                code: CODE_BAD_REQUEST,
                reason: vec![],
            })],
        );
        let insufficent_capacity_msg = build_msg(
            m.transaction_id,
            MessageType::new(METHOD_ALLOCATE, CLASS_ERROR_RESPONSE),
            vec![Box::new(ErrorCodeAttribute {
                code: CODE_INSUFFICIENT_CAPACITY,
                reason: vec![],
            })],
        );

        // 2. The server checks if the 5-tuple is currently in use by an
        //    existing allocation.  If yes, the server rejects the request with
        //    a 437 (Allocation Mismatch) error.
        if self
            .allocation_manager
            .get_allocation(&five_tuple)
            .await
            .is_some()
        {
            let msg = build_msg(
                m.transaction_id,
                MessageType::new(METHOD_ALLOCATE, CLASS_ERROR_RESPONSE),
                vec![Box::new(ErrorCodeAttribute {
                    code: CODE_ALLOC_MISMATCH,
                    reason: vec![],
                })],
            );
            return build_and_send_err(
                &self.conn,
                self.src_addr,
                ERR_RELAY_ALREADY_ALLOCATED_FOR_FIVE_TUPLE.to_owned(),
                &msg,
            )
            .await;
        }

        // 3. The server checks if the request contains a REQUESTED-TRANSPORT
        //    attribute.  If the REQUESTED-TRANSPORT attribute is not included
        //    or is malformed, the server rejects the request with a 400 (Bad
        //    Request) error.  Otherwise, if the attribute is included but
        //    specifies a protocol other that UDP, the server rejects the
        //    request with a 442 (Unsupported Transport Protocol) error.
        let mut requested_transport = RequestedTransport::default();
        if let Err(err) = requested_transport.get_from(m) {
            return build_and_send_err(&self.conn, self.src_addr, err, &bad_request_msg).await;
        } else if requested_transport.protocol != PROTO_UDP {
            let msg = build_msg(
                m.transaction_id,
                MessageType::new(METHOD_ALLOCATE, CLASS_ERROR_RESPONSE),
                vec![Box::new(ErrorCodeAttribute {
                    code: CODE_UNSUPPORTED_TRANS_PROTO,
                    reason: vec![],
                })],
            );
            return build_and_send_err(
                &self.conn,
                self.src_addr,
                ERR_REQUESTED_TRANSPORT_MUST_BE_UDP.to_owned(),
                &msg,
            )
            .await;
        }

        // 4. The request may contain a DONT-FRAGMENT attribute.  If it does,
        //    but the server does not support sending UDP datagrams with the DF
        //    bit set to 1 (see Section 12), then the server treats the DONT-
        //    FRAGMENT attribute in the Allocate request as an unknown
        //    comprehension-required attribute.
        if m.contains(ATTR_DONT_FRAGMENT) {
            let msg = build_msg(
                m.transaction_id,
                MessageType::new(METHOD_ALLOCATE, CLASS_ERROR_RESPONSE),
                vec![
                    Box::new(ErrorCodeAttribute {
                        code: CODE_UNKNOWN_ATTRIBUTE,
                        reason: vec![],
                    }),
                    Box::new(UnknownAttributes(vec![ATTR_DONT_FRAGMENT])),
                ],
            );
            return build_and_send_err(
                &self.conn,
                self.src_addr,
                ERR_NO_DONT_FRAGMENT_SUPPORT.to_owned(),
                &msg,
            )
            .await;
        }

        // 5.  The server checks if the request contains a RESERVATION-TOKEN
        //     attribute.  If yes, and the request also contains an EVEN-PORT
        //     attribute, then the server rejects the request with a 400 (Bad
        //     Request) error.  Otherwise, it checks to see if the token is
        //     valid (i.e., the token is in range and has not expired and the
        //     corresponding relayed transport address is still available).  If
        //     the token is not valid for some reason, the server rejects the
        //     request with a 508 (Insufficient Capacity) error.
        let mut reservation_token_attr = ReservationToken::default();
        if reservation_token_attr.get_from(m).is_ok() {
            let mut even_port = EvenPort::default();
            if even_port.get_from(m).is_ok() {
                return build_and_send_err(
                    &self.conn,
                    self.src_addr,
                    ERR_REQUEST_WITH_RESERVATION_TOKEN_AND_EVEN_PORT.to_owned(),
                    &bad_request_msg,
                )
                .await;
            }
        }

        // 6. The server checks if the request contains an EVEN-PORT attribute.
        //    If yes, then the server checks that it can satisfy the request
        //    (i.e., can allocate a relayed transport address as described
        //    below).  If the server cannot satisfy the request, then the
        //    server rejects the request with a 508 (Insufficient Capacity)
        //    error.
        let mut even_port = EvenPort::default();
        if even_port.get_from(m).is_ok() {
            let mut random_port = 1;

            while random_port % 2 != 0 {
                random_port = match self.allocation_manager.get_random_even_port().await {
                    Ok(port) => port,
                    Err(err) => {
                        return build_and_send_err(
                            &self.conn,
                            self.src_addr,
                            err,
                            &insufficent_capacity_msg,
                        )
                        .await
                    }
                };
            }

            requested_port = random_port;
            reservation_token = rand_seq(8);
        }

        // 7. At any point, the server MAY choose to reject the request with a
        //    486 (Allocation Quota Reached) error if it feels the client is
        //    trying to exceed some locally defined allocation quota.  The
        //    server is free to define this allocation quota any way it wishes,
        //    but SHOULD define it based on the username used to authenticate
        //    the request, and not on the client's transport address.

        // 8. Also at any point, the server MAY choose to reject the request
        //    with a 300 (Try Alternate) error if it wishes to redirect the
        //    client to a different server.  The use of this error code and
        //    attribute follow the specification in [RFC5389].
        let lifetime_duration = allocation_lifetime(m);
        let a = match self
            .allocation_manager
            .create_allocation(
                five_tuple,
                Arc::clone(&self.conn),
                requested_port,
                lifetime_duration,
            )
            .await
        {
            Ok(a) => a,
            Err(err) => {
                return build_and_send_err(
                    &self.conn,
                    self.src_addr,
                    err,
                    &insufficent_capacity_msg,
                )
                .await
            }
        };

        // Once the allocation is created, the server replies with a success
        // response.  The success response contains:
        //   * An XOR-RELAYED-ADDRESS attribute containing the relayed transport
        //     address.
        //   * A LIFETIME attribute containing the current value of the time-to-
        //     expiry timer.
        //   * A RESERVATION-TOKEN attribute (if a second relayed transport
        //     address was reserved).
        //   * An XOR-MAPPED-ADDRESS attribute containing the client's IP address
        //     and port (from the 5-tuple).

        let (src_ip, src_port) = (self.src_addr.ip(), self.src_addr.port());
        let (relay_ip, relay_port) = {
            let a = a.lock().await;
            (a.relay_addr.ip(), a.relay_addr.port())
        };

        let mut response_attrs: Vec<Box<dyn Setter>> = vec![
            Box::new(RelayedAddress {
                ip: relay_ip,
                port: relay_port,
            }),
            Box::new(Lifetime(lifetime_duration)),
            Box::new(XORMappedAddress {
                ip: src_ip,
                port: src_port,
            }),
        ];

        if !reservation_token.is_empty() {
            let reservation_token_vec = reservation_token.as_bytes().to_vec();
            self.allocation_manager
                .create_reservation(reservation_token, relay_port)
                .await;
            response_attrs.push(Box::new(ReservationToken(reservation_token_vec)));
        }

        response_attrs.push(Box::new(message_integrity));
        let msg = build_msg(
            m.transaction_id,
            MessageType::new(METHOD_ALLOCATE, CLASS_SUCCESS_RESPONSE),
            response_attrs,
        );

        build_and_send(&self.conn, self.src_addr, &msg).await
    }

    pub(crate) async fn handle_refresh_request(&mut self, m: &Message) -> Result<(), Error> {
        log::debug!("received RefreshRequest from {}", self.src_addr);

        let message_integrity = self.authenticate_request(m, METHOD_REFRESH).await?;

        let lifetime_duration = allocation_lifetime(m);
        let five_tuple = FiveTuple {
            src_addr: self.src_addr,
            dst_addr: self.conn.local_addr()?,
            protocol: PROTO_UDP,
        };

        if lifetime_duration != Duration::from_secs(0) {
            let a = self.allocation_manager.get_allocation(&five_tuple).await;
            if let Some(a) = a {
                let a = a.lock().await;
                a.refresh(lifetime_duration).await;
            } else {
                return Err(ERR_NO_ALLOCATION_FOUND.to_owned());
            }
        } else {
            self.allocation_manager.delete_allocation(&five_tuple).await;
        }

        build_and_send(
            &self.conn,
            self.src_addr,
            &build_msg(
                m.transaction_id,
                MessageType::new(METHOD_REFRESH, CLASS_SUCCESS_RESPONSE),
                vec![
                    Box::new(Lifetime(lifetime_duration)),
                    Box::new(message_integrity),
                ],
            ),
        )
        .await
    }

    pub(crate) async fn handle_create_permission_request(
        &mut self,
        m: &Message,
    ) -> Result<(), Error> {
        log::debug!("received CreatePermission from {}", self.src_addr);

        let a = self
            .allocation_manager
            .get_allocation(&FiveTuple {
                src_addr: self.src_addr,
                dst_addr: self.conn.local_addr()?,
                protocol: PROTO_UDP,
            })
            .await;

        if let Some(a) = a {
            let message_integrity = self
                .authenticate_request(m, METHOD_CREATE_PERMISSION)
                .await?;
            let mut add_count = 0;

            {
                let a = a.lock().await;
                for attr in &m.attributes.0 {
                    if attr.typ != ATTR_XOR_PEER_ADDRESS {
                        continue;
                    }

                    let mut peer_address = PeerAddress::default();
                    if peer_address.get_from(m).is_err() {
                        add_count = 0;
                        break;
                    }

                    log::debug!(
                        "adding permission for {}",
                        format!("{}:{}", peer_address.ip, peer_address.port)
                    );

                    a.add_permission(Permission::new(SocketAddr::new(
                        peer_address.ip,
                        peer_address.port,
                    )))
                    .await;
                    add_count += 1;
                }
            }

            let mut resp_class = CLASS_SUCCESS_RESPONSE;
            if add_count == 0 {
                resp_class = CLASS_ERROR_RESPONSE;
            }

            build_and_send(
                &self.conn,
                self.src_addr,
                &build_msg(
                    m.transaction_id,
                    MessageType::new(METHOD_CREATE_PERMISSION, resp_class),
                    vec![Box::new(message_integrity)],
                ),
            )
            .await
        } else {
            Err(ERR_NO_ALLOCATION_FOUND.to_owned())
        }
    }

    pub(crate) async fn handle_send_indication(&mut self, m: &Message) -> Result<(), Error> {
        log::debug!("received SendIndication from {}", self.src_addr);

        let a = self
            .allocation_manager
            .get_allocation(&FiveTuple {
                src_addr: self.src_addr,
                dst_addr: self.conn.local_addr()?,
                protocol: PROTO_UDP,
            })
            .await;

        if let Some(a) = a {
            let mut data_attr = Data::default();
            data_attr.get_from(m)?;

            let mut peer_address = PeerAddress::default();
            peer_address.get_from(m)?;

            let msg_dst = SocketAddr::new(peer_address.ip, peer_address.port);

            let has_perm = {
                let a = a.lock().await;
                a.has_permission(&msg_dst).await
            };
            if !has_perm {
                return Err(ERR_NO_PERMISSION.to_owned());
            }

            let a = a.lock().await;
            let l = a.relay_socket.send_to(&data_attr.0, msg_dst).await?;
            if l != data_attr.0.len() {
                Err(ERR_SHORT_WRITE.to_owned())
            } else {
                Ok(())
            }
        } else {
            Err(ERR_NO_ALLOCATION_FOUND.to_owned())
        }
    }

    pub(crate) async fn handle_channel_bind_request(&mut self, m: &Message) -> Result<(), Error> {
        log::debug!("received ChannelBindRequest from {}", self.src_addr);

        let a = self
            .allocation_manager
            .get_allocation(&FiveTuple {
                src_addr: self.src_addr,
                dst_addr: self.conn.local_addr()?,
                protocol: PROTO_UDP,
            })
            .await;

        if let Some(a) = a {
            let bad_request_msg = build_msg(
                m.transaction_id,
                MessageType::new(METHOD_CHANNEL_BIND, CLASS_ERROR_RESPONSE),
                vec![Box::new(ErrorCodeAttribute {
                    code: CODE_BAD_REQUEST,
                    reason: vec![],
                })],
            );

            let message_integrity = self.authenticate_request(m, METHOD_CHANNEL_BIND).await?;
            let mut channel = ChannelNumber::default();
            if let Err(err) = channel.get_from(m) {
                return build_and_send_err(&self.conn, self.src_addr, err, &bad_request_msg).await;
            }

            let mut peer_addr = PeerAddress::default();
            if let Err(err) = peer_addr.get_from(m) {
                return build_and_send_err(&self.conn, self.src_addr, err, &bad_request_msg).await;
            }

            log::debug!(
                "binding channel {} to {}",
                channel,
                format!("{}:{}", peer_addr.ip, peer_addr.port)
            );

            let result = {
                let a = a.lock().await;
                a.add_channel_bind(
                    ChannelBind::new(channel, SocketAddr::new(peer_addr.ip, peer_addr.port)),
                    self.channel_bind_timeout,
                )
                .await
            };
            if let Err(err) = result {
                return build_and_send_err(&self.conn, self.src_addr, err, &bad_request_msg).await;
            }

            return build_and_send(
                &self.conn,
                self.src_addr,
                &build_msg(
                    m.transaction_id,
                    MessageType::new(METHOD_CHANNEL_BIND, CLASS_SUCCESS_RESPONSE),
                    vec![Box::new(message_integrity)],
                ),
            )
            .await;
        } else {
            Err(ERR_NO_ALLOCATION_FOUND.to_owned())
        }
    }

    pub(crate) async fn handle_channel_data(&mut self, c: &ChannelData) -> Result<(), Error> {
        log::debug!("received ChannelData from {}", self.src_addr);

        let a = self
            .allocation_manager
            .get_allocation(&FiveTuple {
                src_addr: self.src_addr,
                dst_addr: self.conn.local_addr()?,
                protocol: PROTO_UDP,
            })
            .await;

        if let Some(a) = a {
            let a = a.lock().await;
            let channel = a.get_channel_addr(&c.number).await;
            if let Some(peer) = channel {
                let l = a.relay_socket.send_to(&c.data, peer).await?;
                if l != c.data.len() {
                    Err(ERR_SHORT_WRITE.to_owned())
                } else {
                    Ok(())
                }
            } else {
                Err(ERR_NO_SUCH_CHANNEL_BIND.to_owned())
            }
        } else {
            Err(ERR_NO_ALLOCATION_FOUND.to_owned())
        }
    }
}
