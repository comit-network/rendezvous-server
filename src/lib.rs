use anyhow::{Context, Result};
use libp2p::identity::ed25519::{Keypair, SecretKey};
use libp2p::ping::{Ping, PingConfig, PingEvent};
use libp2p::rendezvous::Rendezvous;
use libp2p::{rendezvous, NetworkBehaviour};
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use tokio::fs::{DirBuilder, OpenOptions};
use tokio::io::AsyncWriteExt;

pub mod transport;

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

pub async fn load_secret_key_from_file(path: impl AsRef<Path> + Debug) -> Result<SecretKey> {
    let bytes = fs::read(&path)
        .await
        .context(format!("No secret file at {:?}", path))?;
    let secret_key = SecretKey::from_bytes(bytes)?;
    Ok(secret_key)
}

pub async fn generate_secret_key_file(path: PathBuf) -> Result<SecretKey> {
    if let Some(parent) = path.parent() {
        DirBuilder::new()
            .recursive(true)
            .create(parent)
            .await
            .context(format!(
                "Could not create directory for secret file: {:?}",
                parent
            ))?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
        .context(format!("Could not generate secret file at {:?}", &path))?;

    let keypair = Keypair::generate();
    let secret_key = SecretKey::from(keypair);

    file.write_all(secret_key.as_ref()).await?;

    Ok(secret_key)
}
