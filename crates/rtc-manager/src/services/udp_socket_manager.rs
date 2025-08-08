use std::{
    collections::HashMap,
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use dashmap::DashMap;
use parking_lot::RwLock;
use str0m::{Candidate, Input, net::Protocol};
use tokio::sync::mpsc;
use tracing::{error, info, debug};

use crate::{
    errors::RtcError,
    models::params::RtcManagerConfigs,
};

/// Manages a shared UDP socket for all WebRTC traffic
/// Similar to str0m/examples/chat.rs implementation
pub struct UdpSocketManager {
    socket: Arc<UdpSocket>,
    local_addr: SocketAddr,
    /// Address we announce in ICE candidates (may differ in IP from bind address)
    announce_addr: SocketAddr,
    rtc_instances: Arc<DashMap<SocketAddr, Arc<RwLock<str0m::Rtc>>>>,
    shutdown_tx: Option<mpsc::UnboundedSender<()>>,
}

impl UdpSocketManager {
    /// Create a new UDP socket manager
    pub fn new(configs: &RtcManagerConfigs) -> Result<Self, RtcError> {
        // Bind on 0.0.0.0 to receive on all interfaces.
        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| RtcError::IoError(e))?;
        socket.set_nonblocking(true).ok();

        let local_addr = socket.local_addr()
            .map_err(|e| RtcError::IoError(e))?;

        // Determine which IP to announce in ICE candidates.
        let announce_ip = if !configs.public_ip.is_empty() {
            configs.public_ip.parse().unwrap_or(local_addr.ip())
        } else {
            select_host_address()
        };
        let announce_addr = SocketAddr::new(announce_ip, local_addr.port());

        info!("Bound UDP on {} (announce {})", local_addr, announce_addr);

        let socket = Arc::new(socket);
        let rtc_instances = Arc::new(DashMap::new());

        Ok(Self {
            socket,
            local_addr,
            announce_addr,
            rtc_instances,
            shutdown_tx: None,
        })
    }

    /// Start the UDP socket run loop
    pub fn start(&mut self) -> Result<(), RtcError> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel();
        self.shutdown_tx = Some(shutdown_tx);

        let socket = Arc::clone(&self.socket);
        let rtc_instances = Arc::clone(&self.rtc_instances);

        // The run loop is on a separate thread. It only receives and routes packets.
        thread::spawn(move || {
            let mut buf = [0u8; 2000];

            loop {
                // Check for shutdown signal
                if let Ok(_) = shutdown_rx.try_recv() {
                    info!("UDP socket manager shutting down");
                    break;
                }

                // Set a short timeout for socket operations
                if let Err(e) = socket.set_read_timeout(Some(Duration::from_millis(10))) {
                    error!("Failed to set socket timeout: {}", e);
                    continue;
                }

                // Try to receive data
                match socket.recv_from(&mut buf) {
                    Ok((n, source)) => {
                        debug!("Received {} bytes from {}", n, source);

                        // Find the RTC instance for this source
                        if let Some(rtc_entry) = rtc_instances.get(&source) {
                            let rtc = rtc_entry.value();
                            let mut rtc_guard = rtc.write();

                            // Handle the incoming data
                            let input = Input::Receive(
                                Instant::now(),
                                str0m::net::Receive {
                                    source,
                                    destination: socket.local_addr().unwrap(),
                                    contents: buf[..n].to_vec().into(),
                                }
                            );

                            if let Err(e) = rtc_guard.handle_input(input) {
                                error!("Failed to handle input for {}: {}", source, e);
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // Timeout, continue to next iteration
                    }
                    Err(e) => {
                        error!("Socket receive error: {}", e);
                    }
                }

                // Small delay to prevent busy loop
                thread::sleep(Duration::from_millis(1));
            }
        });

        Ok(())
    }

    /// Register an RTC instance with a remote address
    pub fn register_rtc(&self, remote_addr: SocketAddr, rtc: Arc<RwLock<str0m::Rtc>>) {
        self.rtc_instances.insert(remote_addr, rtc);
    }

    /// Unregister an RTC instance
    pub fn unregister_rtc(&self, remote_addr: &SocketAddr) {
        self.rtc_instances.remove(remote_addr);
    }

    /// Send a packet produced by str0m over the shared socket
    pub fn send_transmit(&self, transmit: str0m::net::Transmit) -> std::io::Result<usize> {
        self.socket.send_to(&transmit.contents, transmit.destination)
    }

    /// Get the local socket address
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Create a host candidate for the announced address (chat.rs style)
    pub fn create_host_candidate(&self) -> Result<Candidate, RtcError> {
        Candidate::host(self.announce_addr, Protocol::Udp)
            .map_err(|_| RtcError::InvalidIceCandidate)
    }

    /// Shutdown the UDP socket manager
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for UdpSocketManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Select a host address for announcing in ICE candidates.
/// Try to detect an outward-facing local IP by creating a UDP socket and connecting to a public address.
fn select_host_address() -> std::net::IpAddr {
    // Best-effort detection
    if let Ok(s) = UdpSocket::bind("0.0.0.0:0") {
        let _ = s.connect("8.8.8.8:53"); // no packets are sent, but OS selects an interface
        if let Ok(addr) = s.local_addr() {
            let ip = addr.ip();
            if !ip.is_loopback() && !ip.is_unspecified() {
                return ip;
            }
        }
    }
    // Fallback: 127.0.0.1
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::params::RtcManagerConfigs;

    #[test]
    fn test_udp_socket_manager_creation() {
        let configs = RtcManagerConfigs {
            public_ip: "127.0.0.1".to_string(),
            port_min: 10000,
            port_max: 10010,
        };

        let manager = UdpSocketManager::new(&configs);
        assert!(manager.is_ok());
    }

    #[test]
    fn test_host_candidate_creation() {
        let configs = RtcManagerConfigs {
            public_ip: "127.0.0.1".to_string(),
            port_min: 10000,
            port_max: 10010,
        };

        let manager = UdpSocketManager::new(&configs).unwrap();
        let candidate = manager.create_host_candidate();
        assert!(candidate.is_ok());
    }
}
