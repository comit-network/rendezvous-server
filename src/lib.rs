use anyhow::Result;
use libp2p::core::identity::ed25519::SecretKey;
use libp2p::identify::{Identify, IdentifyEvent};
use libp2p::rendezvous::Rendezvous;
use libp2p::{rendezvous, NetworkBehaviour};

pub mod transport;

pub fn parse_secret_key(s: &str) -> Result<SecretKey> {
    let bytes = s.to_string().into_bytes();
    let secret_key = SecretKey::from_bytes(bytes)?;
    Ok(secret_key)
}

#[derive(Debug)]
pub enum Event {
    Rendezvous(rendezvous::Event),
    Identify(IdentifyEvent),
}

impl From<rendezvous::Event> for Event {
    fn from(event: rendezvous::Event) -> Self {
        Event::Rendezvous(event)
    }
}

impl From<IdentifyEvent> for Event {
    fn from(event: IdentifyEvent) -> Self {
        Event::Identify(event)
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(event_process = false)]
#[behaviour(out_event = "Event")]
pub struct Behaviour {
    identify: Identify,
    pub rendezvous: Rendezvous,
}

impl Behaviour {
    pub fn new(identify: Identify, rendezvous: Rendezvous) -> Self {
        Self {
            identify,
            rendezvous,
        }
    }
}
