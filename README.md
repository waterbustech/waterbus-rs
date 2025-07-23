<div align="center">
<img src="./assets/launcher_icon.png" width="180px" height=auto alt="Waterbus SFU"/>
</div>

<h2 align="center">Waterbus</h2>

<div align="center">
    <a href="https://discord.gg/mfrWVefU"><img src="https://img.shields.io/badge/-Discord-424549?style=social&logo=discord" height=25></a>
    &nbsp;
    <a href="https://t.me/+0LckY3ZY2k00NzVl"><img src="https://img.shields.io/badge/-Telegram-red?style=social&logo=telegram" height=25></a>
    &nbsp;
    <a href="https://twitter.com/waterbustech"><img src="https://img.shields.io/badge/-Twitter-red?style=social&logo=x" height=25></a>
</div>

<p align="center">
  <a href="https://docs.waterbus.tech">Website</a> &bull;
  <a href="https://github.com/waterbustech/waterbus/wiki">Wiki</a> &bull;
  <a href="https://github.com/waterbustech/waterbus/blob/main/LICENSE">License</a>
</p>

**[Waterbus](https://waterbus.tech/)** is an open-source platform for building **scalable, real-time conferencing** systems using **WebRTC**. It enables low-latency streaming of **video, audio, and data** for applications like video chat, collaborative tools, and multiplayer experiences.

The server is written in **Rust**, using the native [WebRTC.rs](https://github.com/webrtc-rs/webrtc) implementation to maximize performance, safety, and control over media flow.

![Waterbus SFU Simulcast](./assets/waterbusrs.svg)

## ✨ Features

- 📚 **Publish/Subscribe Media Tracks**
  
    Flexible pub/sub model to let users publish and subscribe to video/audio streams dynamically.

- 🎥 **Simulcast with Bandwidth Awareness**

    Uses simulcast to send multiple video layers per publisher. Waterbus leverages **Transport-CC (Transport-wide Congestion Control)** feedback to monitor subscriber bandwidth and forwards only the most suitable video quality for each participant.

- 📈 **Scalable by Design**

    Optimized to scale horizontally, `Waterbus` supports thousands of concurrent users with efficient SFU architecture and Redis-based signaling.

- 💬 **Built-in Chat Messaging**

    Integrated text messaging alongside media streams — perfect for chat, reactions, or control messages.

- 🌍 **Cross-Platform Compatibility**

   Works across browsers, native apps using WebRTC standards.

## 📦 Requirements

This project uses the following technologies:

- [Rust](https://www.rust-lang.org/) `1.82+`
- [PostgreSQL](https://www.postgresql.org/) `16+`
- [Redis](https://redis.io/) `7+`
- [Diesel CLI](https://diesel.rs/)

- Install `Diesel CLI`:

```bash
cargo install diesel_cli --no-default-features --features postgres
```

## ⚡️ Quick Start

Get up and running with Waterbus in just a few steps.

### 🛠 Local Build

- Start by cloning the Waterbus server repository:

```bash
git clone https://github.com/waterbustech/waterbus-rs.git 
cd waterbus-rs
```

- Rename the example .env file to configure your local environment:

```bash
mv example.env .env
```

- Initialize the database

```bash
diesel migration run
```

- Build & Run the Server

```bash
cargo run --release
```

### 🐳 Run with Docker

If you prefer to use Docker:

- Build the Docker image:

```bash
sudo docker build --platform=linux/amd64 -t <image-name> .
```

- Run the container with necessary ports exposed:

```bash
sudo docker run --env-file .env -p 5998:5998 -p 5998:5998/udp -p 19200-19250:19200-19250/udp <image-name>
```

## ❓ Why We Migrated from NestJS to Rust

While [NestJS](https://nestjs.com) served us well in the early stages, we encountered limitations when scaling up real-time media workloads:

- **Performance:** Rust provides native-level speed, predictable memory management, and zero-cost abstractions — ideal for a real-time system.
- **Control:** Media routing, simulcast, congestion control, and fine-grained memory management are all more accessible and performant in a systems language.
- **Safety:** Rust’s ownership model ensures memory safety and eliminates entire classes of bugs (nulls, races, leaks).
- **Concurrency:** Rust's async ecosystem (Tokio, etc.) is well-suited to handling thousands of concurrent connections with minimal overhead.

This migration enables us to build a **high-performance WebRTC SFU**, tailored exactly to our needs.

## 📡 How Does Waterbus SFU Simulcast Work?

Simulcast allows a publisher to send **multiple versions** of the same video stream at different resolutions and bitrates (e.g., 1080p, 720p, 360p).

Waterbus uses the following approach:

1. **Client Encodes Multiple Layers** → Sender sends low, mid, and high-quality streams.
2. **Transport-CC Feedback from Receiver** → Each subscriber sends Transport-wide Congestion Control (Transport-CC) reports.
3. **Adaptive Stream Forwarding** → The server uses Transport-CC to dynamically forward the most suitable layer for each subscriber, ensuring the best quality without overloading their connection.

This provides a **responsive, bandwidth-efficient experience**, especially in group calls with diverse devices and network conditions.

## 💙 Show Your Support

If you like what we're building, consider showing your love by giving us a ⭐ on [Github](https://github.com/waterbustech/waterbus-rs/stargazers)!

Also, follow [maintainers](https://github.com/lambiengcode) on GitHub for our next creations!

## 🤝  Contribute to Waterbus

We welcome all contributions — big or small!

Whether it’s fixing a bug, suggesting a feature, or improving documentation, your input helps `Waterbus` grow.

- Fork the repo, make your changes, and open a [pull request](https://github.com/waterbustech/waterbus-rs/pulls)
- Spot an issue? [Open a GitHub Issue](https://github.com/waterbustech/waterbus-rs/issues) — we’d love to hear from you
  
Let’s build the future of real-time communication together 🚀

## 🔗 Useful Links

- 📢 [waterbus.tech](http://waterbus.tech/): Home page to introduce products and features.
- 🌍 [meet.waterbus.tech](http://meet.waterbus.tech/): Demo of `Waterbus` for online meetings
- 📖 [developer docs](http://docs.waterbus.tech/): Everything you need to integrate and extend Waterbus

## 📜 License

Waterbus is open source under the [Apache 2.0 License](https://www.apache.org/licenses/LICENSE-2.0).
Use it freely, fork it, build amazing things.

## 📬 Get in Touch

Got questions, feedback, or ideas? Reach out anytime at lambiengcode@gmail.com

Made with 💙 by the Waterbus Team.