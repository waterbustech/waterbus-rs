# Waterbus RTC Manager Architecture Summary

## Overview

Waterbus is a real-time video conferencing system with two WebRTC management crates representing different architectural generations:

- **`crates/webrtc-manager`** - Legacy implementation using `webrtc-rs`
- **`crates/rtc-manager`** - Modern implementation using `str0m`

## RTC Manager (Current - v0.3.0)

### Core Purpose
The `rtc-manager` crate serves as the core WebRTC management library for waterbus, providing:
- Real-time communication engine for video conferencing
- SFU (Selective Forwarding Unit) operations with Publisher-Subscriber architecture
- P2P and SFU hybrid topology support
- Room-based video conferencing functionality

### Key Architecture Features

#### 1. **WebRTC Library Migration**
- **From**: `webrtc-rs` (async/await based)
- **To**: `str0m` (thread-based synchronous)
- **Benefits**: Simplified ICE handling, native SDP processing, better performance

#### 2. **Threading Model**
- **Thread-based synchronous architecture** using `std::thread`
- Replaced async/await with synchronous code
- Uses `RwLock` and `DashMap` for thread-safe operations
- No async runtime overhead

#### 3. **Publisher-Subscriber Architecture**
- Each `Publisher` contains a `DashMap` of its subscribers
- Direct media forwarding through str0m's `Writer` API
- Simplified media forwarding and better scalability

#### 4. **Automatic ICE Candidate Handling**
- str0m handles ICE candidates automatically
- Eliminated manual ICE candidate exchange complexity
- Reduced potential failure points

### Core Components

#### **RtcManager** (`src/rtc_manager.rs`)
- Central coordinator for all WebRTC operations
- Key methods: `join_room()`, `subscribe()`, `leave_room()`
- Media control: `set_audio_enabled()`, `set_video_enabled()`
- Thread-safe using `DashMap` and `RwLock`

#### **Room** (`src/entities/room.rs`)
- Manages `Publisher` instances within a room
- P2P/SFU hybrid topology logic
- SDP caching for P2P connections
- Synchronous `run_udp_loop()` using std::thread

#### **Publisher-Subscriber Model**
- **Publishers**: Handle incoming media from participants
- **Subscribers**: Receive and forward media to other participants
- Media forwarding uses str0m's optimized Writer API

### Key Features
1. **Hybrid Topology**: P2P (direct) and SFU (server-forwarded) modes
2. **Real-time Media**: Audio/video streaming with keyframe management
3. **E2EE Support**: End-to-end encryption capabilities
4. **Screen Sharing**: Built-in screen sharing functionality
5. **HLS Streaming**: Support for HLS live stream subscriptions
6. **Native SDP Handling**: Using str0m's `from_sdp_string()` and `to_string()`

### Integration
Used by the **SFU service** (`sfu/src/application/sfu_grpc_service.rs`):
- Wraps `RtcManager` in gRPC service endpoints
- Handles WebRTC signaling through protocol buffers
- Manages participant lifecycle in distributed rooms
- Integrates with dispatcher service for room coordination

## WebRTC Manager (Legacy - v0.2.0)

### Architecture
- **WebRTC Library**: `webrtc-rs`
- **Concurrency**: Async/await with tokio
- **SDP Handling**: JSON-based serialization/deserialization
- **ICE Management**: Manual ICE candidate exchange
- **Track Management**: Automatic `track_id` to `client_track_id` mapping

### Key Differences from RTC Manager
1. **Async Architecture**: Uses tokio for concurrency vs. std::thread
2. **Manual ICE Handling**: Requires explicit ICE candidate management
3. **JSON SDP**: Uses serde_json for SDP handling vs. native str0m format
4. **Complex Track Management**: Manual creation of Media, Track, and ForwardTrack structs
5. **Separate Publishers/Subscribers**: Room-level separation vs. publisher-contained subscribers

## Migration Benefits

### Technical Improvements
1. **Simplified Architecture**: Removed manual ICE handling, streamlined media flow
2. **Better Performance**: Direct media forwarding, reduced allocations, faster SDP parsing
3. **Enhanced Maintainability**: Cleaner separation, explicit track management with `mid`
4. **Improved Scalability**: Better publisher-subscriber model, efficient thread-safe operations

### Development Experience
- Synchronous code is easier to debug than async
- Native SDP handling without JSON overhead
- Thread-based architecture scales better for WebRTC
- Simplified Rtc lifecycle management

## Commands

### Build & Test
```bash
# Build specific crate
cargo check -p rtc-manager
cargo check -p webrtc-manager

# Test migration example
cargo run --example str0m_migration

# Build entire project
cargo build

# Run tests
cargo test
```

### Development
```bash
# Format code
cargo fmt

# Check lints
cargo clippy

# Run SFU service
cargo run --bin sfu

# Run signalling service
cargo run --bin signalling
```

## Migration Status
🚧 **IN PROGRESS**: Migration from webrtc-rs to str0m is ongoing with ICE connectivity issues

### Current Status
- ✅ SDP negotiation working
- ✅ Room creation and UDP binding functional
- ✅ ICE candidates being added
- ❌ ICE connection state failing after candidate exchange

### Fixed Issues (Phase 1 Complete)
- ✅ **UDP Run Loop**: Re-enabled UDP thread in room creation
- ✅ **Packet Routing**: UDP packets now routed to correct publishers using `rtc.accepts()`  
- ✅ **Public IP**: Now uses config public_ip instead of Docker internal IP (172.18.0.1)
- ✅ **ICE Events**: Added proper ICE connection state change logging

### Phase 1 Implementation Changes
1. **Room Creation**: UDP loop thread automatically spawned when room is created
2. **UDP Input**: Incoming packets checked against each publisher via `rtc.accepts()` and routed properly
3. **Host Candidates**: Room uses public IP from config for reachable host candidates
4. **Event Handling**: ICE connection state changes properly logged

### Remaining Work
- **Phase 2**: Forward ICE candidates to existing callback system (if needed)
- **Phase 3**: Add graceful shutdown and optimize socket handling

The critical ICE connectivity issues should now be resolved with these fixes.

---

# str0m WebRTC Library

## Overview
**str0m** is a Sans I/O WebRTC implementation in Rust that serves as the core WebRTC engine for waterbus's rtc-manager. It provides a synchronous, thread-based architecture without internal async tasks or network I/O.

## Key Architecture Principles

### 1. **Sans I/O Design**
- No internal network operations - all I/O is external
- No internal threads or async tasks
- All operations driven by public API calls
- Time and network input/output are external inputs

### 2. **State Machine Architecture**
- Enormous state machine driven by different input types:
  - User operations (sending media, data channel data)
  - Network input (UDP packets)
  - Timeouts (for internal timing)

### 3. **No Standard WebRTC API**
- Deliberately avoids RTCPeerConnection callback model
- Uses `&mut self` pattern instead of callbacks/channels
- Eliminates need for `Arc`, `Mutex`, or other locks
- Events returned as values rather than callbacks

## Core API Components

### **Rtc Instance**
- Central WebRTC peer connection manager
- Main methods:
  - `poll_output()` - Get next output (transmit/event/timeout)
  - `handle_input()` - Process input (network/timeout)
  - `sdp_api()` - SDP offer/answer negotiation
  - `writer()` - Get media writer for sending

### **Output Types**
1. **Output::Transmit** - UDP data to send to remote peer
2. **Output::Event** - Media data, connection state changes, etc.
3. **Output::Timeout** - When next input is expected

### **Input Types**
1. **Input::Receive** - Incoming UDP packet data
2. **Input::Timeout** - Time advancement for internal state

## Media Handling

### **Sample Level API (Default)**
- Works with complete audio frames or video frames
- `Event::MediaData` - Receive complete samples
- `Writer::write()` - Send complete samples
- `Writer::request_keyframe()` - Request keyframes

### **RTP Level API**
- Lower-level RTP packet handling
- `Event::RtpPacket` - Individual RTP packets
- `StreamTx::write_rtp()` - Send RTP packets
- Requires deeper WebRTC/RTP knowledge

## Time Management

### **Three Time Concepts**
1. **"Now" Time** (`Instant`) - Drives internal state forward
2. **Media Time** - RTP timestamps (90kHz video, 48kHz audio)
3. **Wallclock Time** - NTP time when media was produced

### **No Internal Clock**
- str0m never calls `Instant::now()` internally
- All time is external input via `handle_input(Input::Timeout(now))`
- Enables testing faster than realtime

## SDP Negotiation

### **Non-Standard Compliance**
- Breaks SDP spec in practical ways like other libraries
- Treats payload type SHOULD as MUST (like browsers)
- Single PT/codec config rather than separate send/receive
- Special handling for direction changes from Inactive

### **API Methods**
- `accept_offer()` - Handle incoming SDP offers
- `accept_answer()` - Handle SDP answers to our offers
- `add_media()` - Add media tracks before creating offers

## ICE and Network

### **ICE Candidate Management**
- Automatic ICE candidate handling
- Add local candidates via `add_local_candidate()`
- TURN/STUN handling is external (Sans I/O)
- Supports IPv4, IPv6, UDP, TCP

### **Network Interface Enumeration**
- Outside str0m scope (Sans I/O pattern)
- User responsible for discovering local IPs
- TURN considered similar to local interface discovery

## Crypto Backends

### **Two Options**
1. **OpenSSL** (default) - Works on all platforms including Windows
2. **WinCrypto** - Windows-only using native APIs

### **Configuration**
```rust
use str0m::config::CryptoProvider;
CryptoProvider::WinCrypto.install_process_default();
```

## Error Handling Philosophy

### **Fail-Fast Approach**
- Panics on internal invariant violations (bugs)
- Returns errors for user input issues
- Every `unwrap()` has code comment explaining safety
- Panics indicate bugs that should be reported

### **Panic Safety**
- No internal threads/locks/async tasks
- Safe to use `catch_unwind()` to discard faulty instances
- No lock poisoning risks

## Usage Examples

### **Basic Setup**
```rust
let mut rtc = Rtc::new();
let candidate = Candidate::host("1.2.3.4:5000".parse().unwrap(), "udp").unwrap();
rtc.add_local_candidate(candidate);
```

### **Run Loop Pattern**
```rust
loop {
    match rtc.poll_output().unwrap() {
        Output::Timeout(timeout) => {
            // Wait for timeout or network input
            let duration = timeout - Instant::now();
            socket.set_read_timeout(Some(duration)).unwrap();
            // Handle socket read...
        }
        Output::Transmit(data) => {
            socket.send_to(&data.contents, data.destination).unwrap();
        }
        Output::Event(event) => {
            // Handle media data, connection state, etc.
        }
    }
}
```

### **Media Sending**
```rust
let writer = rtc.writer(mid).unwrap();
let pt = writer.payload_params().nth(0).unwrap().pt();
writer.write(pt, wallclock, media_time, data).unwrap();
```

## Integration in Waterbus

### **Why str0m for rtc-manager?**
1. **Simplified ICE handling** - Automatic instead of manual
2. **Better performance** - Direct media forwarding, fewer allocations  
3. **Thread-safe design** - No async complexity
4. **Native SDP handling** - Faster than JSON serialization
5. **Explicit track management** - Using `mid` instead of auto-mapping

### **Migration Benefits**
- Synchronous code easier to debug than async
- Thread-based architecture scales better for WebRTC
- Reduced complexity in SFU scaling
- Better resource management

## Available Examples
- `cargo run --example chat` - Multi-browser SFU example
- `cargo run --example http-post` - Simple media receiver
- Real-world usage: [BitWHIP](https://github.com/bitwhip/bitwhip) CLI tool

## Testing Commands
```bash
# Build str0m
cargo check -p str0m

# Run examples (requires TLS)
cargo run --example chat
cargo run --example http-post

# Test waterbus integration
cargo run --example str0m_migration
```
