use anyhow::Result;
use libp2p::core::identity::ed25519::SecretKey;
use libp2p::ping::{Ping, PingConfig, PingEvent};
use libp2p::rendezvous::Rendezvous;
use libp2p::{rendezvous, NetworkBehaviour};
use std::fs;
use std::path::Path;
use std::time::Duration;

pub mod transport;

pub fn parse_secret_key(s: &str) -> Result<SecretKey> {
    let bytes = s.to_string().into_bytes();
    let secret_key = SecretKey::from_bytes(bytes)?;
    Ok(secret_key)
}

#[derive(Debug)]
pub enum Event {
    Rendezvous(rendezvous::Event),
    Ping(PingEvent),
}

impl From<rendezvous::Event> for Event {
    fn from(event: rendezvous::Event) -> Self {
        Event::Rendezvous(event)
    }
}

impl From<PingEvent> for Event {
    fn from(event: PingEvent) -> Self {
        Event::Ping(event)
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(event_process = false)]
#[behaviour(out_event = "Event")]
pub struct Behaviour {
    ping: Ping,
    pub rendezvous: Rendezvous,
}

impl Behaviour {
    pub fn new(rendezvous: Rendezvous) -> Self {
        Self {
            // TODO: Remove Ping behaviour once https://github.com/libp2p/rust-libp2p/issues/2109 is fixed
            // interval for sending Ping set to 24 hours
            ping: Ping::new(
                PingConfig::new()
                    .with_keep_alive(false)
                    .with_interval(Duration::from_secs(86_400)),
            ),
            rendezvous,
        }
    }
}

pub fn load_secret_key_from_file(path: impl AsRef<Path>) -> Result<SecretKey> {
    let bytes = fs::read(path)?;
    let secret_key = SecretKey::from_bytes(bytes)?;
    Ok(secret_key)
}
