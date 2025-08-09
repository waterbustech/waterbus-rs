use std::collections::VecDeque;
use std::net::{IpAddr, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use systemstat::{Platform, System};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use str0m::net::Receive;
use str0m::{net::Protocol, Candidate, Event, Input, Output, Rtc};

use crate::errors::RtcError;
use crate::models::rtc_dto::RtcManagerConfig;

#[derive(Clone)]
pub struct RtcRegistration {
    pub id: String,
    pub rtc: Arc<RwLock<Rtc>>,
    pub event_tx: mpsc::UnboundedSender<Event>,
}

pub struct RtcUdpRuntime {
    socket: UdpSocket,
    clients: Arc<DashMap<String, RtcRegistration>>, // id -> registration
    cancel: CancellationToken,
}

static RUNTIME: OnceCell<Arc<RtcUdpRuntime>> = OnceCell::new();

pub fn select_host_address() -> IpAddr {
    let system = System::new();
    let networks = system.networks().unwrap();

    for net in networks.values() {
        for n in &net.addrs {
            if let systemstat::IpAddr::V4(v) = n.addr {
                if !v.is_loopback() && !v.is_link_local() && !v.is_broadcast() {
                    return IpAddr::V4(v);
                }
            }
        }
    }

    panic!("Found no usable network interface");
}

impl RtcUdpRuntime {
    pub fn init(_config: RtcManagerConfig) -> Result<(), RtcError> {
        if RUNTIME.get().is_some() {
            return Ok(());
        }

        let host_addr = select_host_address();
        let socket = UdpSocket::bind(format!("{host_addr}:0")).expect("binding a random UDP port");

        socket
            .set_nonblocking(false)
            .map_err(|_| RtcError::FailedToCreateOffer)?;

        let runtime = Arc::new(RtcUdpRuntime {
            socket,
            clients: Arc::new(DashMap::new()),
            cancel: CancellationToken::new(),
        });

        let runtime_clone = Arc::clone(&runtime);
        std::thread::spawn(move || {
            if let Err(e) = runtime_clone.run_loop() {
                tracing::error!("RtcUdpRuntime loop exited with error: {:?}", e);
            }
        });

        RUNTIME.set(runtime).ok();
        Ok(())
    }

    pub fn global() -> Arc<RtcUdpRuntime> {
        RUNTIME
            .get()
            .expect("RtcUdpRuntime not initialized")
            .clone()
    }

    pub fn register_rtc(
        &self,
        id: String,
        rtc: Arc<RwLock<Rtc>>,
        event_tx: mpsc::UnboundedSender<Event>,
    ) -> Result<(), RtcError> {
        // Add a host candidate that matches the runtime socket
        let addr = self
            .socket
            .local_addr()
            .map_err(|_| RtcError::InvalidIceCandidate)?;

        {
            let mut rtc_lock = rtc.write();
            if let Ok(c) = Candidate::host(addr, Protocol::Udp) {
                rtc_lock.add_local_candidate(c);
            }
        }

        let reg = RtcRegistration {
            id: id.clone(),
            rtc,
            event_tx,
        };
        self.clients.insert(id, reg);
        Ok(())
    }

    pub fn unregister_rtc(&self, id: &str) {
        self.clients.remove(id);
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
    }

    /// Build a host Candidate for the runtime's bound UDP socket
    pub fn host_candidate(&self) -> Option<Candidate> {
        let addr = self.socket.local_addr().ok()?;
        Candidate::host(addr, Protocol::Udp).ok()
    }

    fn run_loop(&self) -> Result<(), RtcError> {
        let mut buf = vec![0u8; 2000];
        let mut to_propagate: VecDeque<(String, Output)> = VecDeque::new();

        loop {
            if self.cancel.is_cancelled() {
                break;
            }

            // Clean-up is implicit as disconnected RTCs should be unregistered by owners.

            // Poll clients for output and compute timeout
            let mut timeout = Instant::now() + Duration::from_millis(100);

            for entry in self.clients.iter() {
                let id = entry.key().clone();
                let rtc_arc = entry.rtc.clone();

                // Drain outputs until timeout is returned
                loop {
                    let output = {
                        let mut rtc = rtc_arc.write();
                        rtc.poll_output()
                    };

                    match output {
                        Ok(Output::Timeout(t)) => {
                            timeout = timeout.min(t);
                            break;
                        }
                        Ok(o) => {
                            to_propagate.push_back((id.clone(), o));
                            continue;
                        }
                        Err(e) => {
                            tracing::warn!("RTC poll_output error: {:?}", e);
                            break;
                        }
                    }
                }
            }

            // Handle any pending outputs
            while let Some((id, out)) = to_propagate.pop_front() {
                self.handle_output(&id, out)?;
            }

            // Setup socket timeout
            let duration = (timeout - Instant::now()).max(Duration::from_millis(1));
            self.socket
                .set_read_timeout(Some(duration))
                .map_err(|_| RtcError::FailedToCreateOffer)?;

            // Read from socket and dispatch to the accepting RTC
            if let Some(input) = Self::read_socket_input(&self.socket, &mut buf) {
                // Find first accepting client
                let mut accepted = false;
                for entry in self.clients.iter() {
                    let rtc_arc = entry.rtc.clone();
                    let accepts = {
                        let rtc = rtc_arc.read();
                        rtc.accepts(&input)
                    };
                    if accepts {
                        let mut rtc = rtc_arc.write();
                        // We need to feed the exact Input; recreate from buffer instead of clone
                        if let Err(e) = rtc.handle_input(input) {
                            tracing::warn!("RTC handle_input failed: {:?}", e);
                        }
                        accepted = true;
                        break;
                    }
                }
                if !accepted {
                    tracing::debug!("No RTC accepted UDP input");
                }
            }

            // Drive time forward
            let now = Instant::now();
            for entry in self.clients.iter() {
                let rtc_arc = entry.rtc.clone();
                let mut rtc = rtc_arc.write();
                let _ = rtc.handle_input(Input::Timeout(now));
            }
        }

        Ok(())
    }

    fn handle_output(&self, id: &str, output: Output) -> Result<(), RtcError> {
        match output {
            Output::Transmit(transmit) => {
                if let Err(e) = self
                    .socket
                    .send_to(&transmit.contents, transmit.destination)
                {
                    tracing::warn!("UDP send_to failed: {:?}", e);
                }
            }
            Output::Timeout(_) => { /* handled in run loop */ }
            Output::Event(e) => {
                if let Some(reg) = self.clients.get(id) {
                    let _ = reg.event_tx.send(e);
                }
            }
        }
        Ok(())
    }

    fn read_socket_input<'a>(socket: &UdpSocket, buf: &'a mut Vec<u8>) -> Option<Input<'a>> {
        use std::io::ErrorKind;

        buf.resize(2000, 0);
        match socket.recv_from(buf) {
            Ok((n, source)) => {
                buf.truncate(n);
                let Ok(contents) = buf.as_slice().try_into() else {
                    return None;
                };
                Some(Input::Receive(
                    Instant::now(),
                    Receive {
                        proto: Protocol::Udp,
                        source,
                        destination: socket.local_addr().unwrap(),
                        contents,
                    },
                ))
            }
            Err(e) => match e.kind() {
                ErrorKind::WouldBlock | ErrorKind::TimedOut => None,
                _ => {
                    tracing::warn!("UdpSocket read failed: {:?}", e);
                    None
                }
            },
        }
    }
}
