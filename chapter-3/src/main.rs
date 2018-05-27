// Copyright 2018 Pierre Krieger
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! # Chapter 3
//!
//! The goal of this chapter is to take the code of chapter 2 and make it compile and run inside
//! of a browser!
//!
//! In order to run this code in the browser, follow these steps:
//!
//! - Install docker if you haven't done so yet.
//! - Create a docker container with the image `tomaka/rustc-emscripten`. This can be done by
//!   running `docker run --rm -it -v `pwd`:/usr/code -w /usr/code tomaka/rustc-emscripten` from
//!   the root of this repository.
//! - From inside the container, go to the `chapter-3` directory and run
//!   `cargo build --target=asmjs-unknown-emscripten`.
//! - Open the `browser.html` file included in this crate in your browser. It should automatically
//!   find the generated JavaScript code.
//!
//! In addition to `browser.html`, you are also given a file `platform.rs`. This file contains
//! platform-independant code that allows you to run an events loop and receive messages from stdin
//! in a cross-plaform way. See the usage in the `main()` function below.
//!
//! The browser doesn't support dialing to a TCP port. The only protocol that is allowed is
//! websockets. Good news, however! The `build_transport()` method in the `platform` module
//! automatically builds a transport that supports websockets. To use them, instead of dialing
//! `/ip4/1.2.3.4/tcp/1000`, you can dial `/ip4/1.2.3.4/tcp/1000/ws`.
//!
//! Additionally, please note that the browser doesn't support listening on any connection (even
//! websockets). Calling `listen_on` will trigger an error at runtime. You can use
//! `if cfg!(not(target_os = "emscripten")) { ... }` to listen only when outside of the browser.
//!
//! Good luck!

extern crate futures;
extern crate tokio_io;

#[cfg(target_os = "emscripten")]
#[macro_use]
extern crate stdweb;


extern crate libp2p;
extern crate rand;
//extern crate tokio_stdin;

use futures::{Future, Stream};

use libp2p::{Multiaddr, PeerId};
use libp2p::core::Transport;
use libp2p::floodsub::{FloodSubUpgrade, FloodSubController, TopicBuilder};

mod platform;

fn main() {
    // The `PlatformSpecific` object allows you to handle the transport and stdin in a
    // cross-platform manner.
    let platform = platform::PlatformSpecific::default();

    // This builds an implementation of the `Transport` trait (similar to the `TcpConfig` object in
    // earlier chapters).
    let transport = platform.build_transport();

    let (floodsub_upgrade, floodsub_rx) = {
        let key = (0..2048).map(|_| rand::random::<u8>()).collect::<Vec<_>>();
        FloodSubUpgrade::new(PeerId::from_public_key(&key))
    };
    let upgraded_transport = transport
        .with_upgrade(floodsub_upgrade.clone());

    let upgr_trans_with_muxing = upgraded_transport.with_dummy_muxing();
    let (swarm_controller, swarm_future) = libp2p::swarm(
        upgr_trans_with_muxing.clone(),
        |future, _remote_addr| {
            future
        });

    if cfg!(not(target_os = "emscripten")) {
        let listen_multiaddr: Multiaddr = "/ip4/0.0.0.0/tcp/4242/ws"
            .parse()
            .expect("failed to parse multiaddress");

        // Let's use the swarm to listen, instead of the raw transport.
        let actual_multiaddr = swarm_controller.listen_on(listen_multiaddr).expect("failed to listen");
        println!("Now listening on {}", actual_multiaddr);
    }

    let topic = TopicBuilder::new("workshop-chapter2-topic")
        .build();

    let floodsub_controller = FloodSubController::new(&floodsub_upgrade);

    floodsub_controller.subscribe(&topic);

    let floodsub_rx = floodsub_rx
        .for_each(|msg| {
            if let Ok(msg) = String::from_utf8(msg.data) {
                println!("> {}", msg);
            } else {
                println!("Received non-utf8 message");
            }

            Ok(())
        });

    let dial_multiaddr: Multiaddr = "/ip4/127.0.0.1/tcp/4242/ws"
        .parse()
        .expect("failed to parse multiaddress");
    swarm_controller.dial(dial_multiaddr, upgr_trans_with_muxing.clone()).expect("Failed to dial");

    // This builds a stream of messages coming from stdin.
    let stdin = platform.stdin();

    // Insert your code here!
    let stdin_future = stdin.for_each(move |msg| {
        floodsub_controller.publish(&topic, msg.into_bytes());

        Ok(())
    });

    // Instead of `core.run()`, use `platform.run()`.
    let final_future = swarm_future
        .select(floodsub_rx).map_err(|(err, _)| err).and_then(|(_, n)| n)
        .select(stdin_future).map_err(|(err, _)| err).and_then(|(_, n)| n);
    platform.run(final_future);
}
