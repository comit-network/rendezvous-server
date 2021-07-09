use anyhow::{Context, Result};
use futures::StreamExt;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::Boxed;
use libp2p::core::upgrade::{SelectUpgrade, Version};
use libp2p::dns::TokioDnsConfig;
use libp2p::identity::ed25519;
use libp2p::mplex::MplexConfig;
use libp2p::noise::{NoiseConfig, X25519Spec};
use libp2p::ping::{Ping, PingConfig, PingEvent};
use libp2p::rendezvous::{Config, Event as RendezvousEvent, Rendezvous};
use libp2p::swarm::{SwarmBuilder, SwarmEvent};
use libp2p::tcp::TokioTcpConfig;
use libp2p::yamux::YamuxConfig;
use libp2p::{identity, noise, rendezvous, Multiaddr, PeerId, Swarm, Transport};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use structopt::StructOpt;
use tokio::fs;
use tokio::fs::{DirBuilder, OpenOptions};
use tokio::io::AsyncWriteExt;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::FmtSubscriber;

#[derive(Debug, StructOpt)]
struct Cli {
    #[structopt(
        long,
        help = "Path to the file that contains the secret key of the rendezvous server's identity keypair"
    )]
    secret_file: PathBuf,
    #[structopt(
        long,
        help = "Set this flag to generate a secret file at the path specified by the --secret-file argument"
    )]
    generate_secret: bool,
    #[structopt(long)]
    port: u16,
    #[structopt(long, help = "Format logs as JSON")]
    json: bool,
    #[structopt(
        long,
        help = "Don't include timestamp in logs. Useful if captured logs already get timestamped, e.g. through journald."
    )]
    no_timestamp: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::from_args();

    init_tracing(LevelFilter::INFO, cli.json, cli.no_timestamp);

    let secret_key = match cli.generate_secret {
        true => {
            let secret_key = ed25519::SecretKey::generate();
            write_secret_key_to_file(&secret_key, cli.secret_file).await?;

            secret_key
        }
        false => load_secret_key_from_file(&cli.secret_file).await?,
    };
    let identity = identity::Keypair::Ed25519(secret_key.into());

    let mut swarm = create_swarm(identity)?;

    tracing::info!(peer_id=%swarm.local_peer_id(), "Rendezvous server peer id");

    swarm
        .listen_on(
            format!("/ip4/0.0.0.0/tcp/{}", cli.port)
                .parse()
                .expect("static string is valid MultiAddress"),
        )
        .context("Failed to initialize listener")?;

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::Behaviour(Event::Rendezvous(RendezvousEvent::PeerRegistered {
                peer,
                registration,
            })) => {
                tracing::info!(%peer, namespace=%registration.namespace, addresses=?registration.record.addresses(), ttl=registration.ttl,  "Peer registered");
            }
            SwarmEvent::Behaviour(Event::Rendezvous(RendezvousEvent::PeerNotRegistered {
                peer,
                namespace,
                error,
            })) => {
                tracing::info!(%peer, %namespace, ?error, "Peer failed to register");
            }
            SwarmEvent::Behaviour(Event::Rendezvous(RendezvousEvent::RegistrationExpired(
                registration,
            ))) => {
                tracing::info!(peer=%registration.record.peer_id(), namespace=%registration.namespace, addresses=%Addresses(registration.record.addresses()), ttl=registration.ttl, "Registration expired");
            }
            SwarmEvent::Behaviour(Event::Rendezvous(RendezvousEvent::PeerUnregistered {
                peer,
                namespace,
            })) => {
                tracing::info!(%peer, %namespace, "Peer unregistered");
            }
            SwarmEvent::Behaviour(Event::Rendezvous(RendezvousEvent::DiscoverServed {
                enquirer,
                ..
            })) => {
                tracing::info!(peer=%enquirer, "Discovery served");
            }
            _ => {}
        }
    }
}

fn init_tracing(level: LevelFilter, json_format: bool, no_timestamp: bool) {
    if level == LevelFilter::OFF {
        return;
    }

    let is_terminal = atty::is(atty::Stream::Stderr);

    let builder = FmtSubscriber::builder()
        .with_env_filter(format!("rendezvous_server={}", level))
        .with_writer(std::io::stderr)
        .with_ansi(is_terminal)
        .with_timer(ChronoLocal::with_format("%F %T".to_owned()))
        .with_target(false);

    if json_format {
        builder.json().init();
        return;
    }

    if no_timestamp {
        builder.without_time().init();
        return;
    }
    builder.init();
}

async fn load_secret_key_from_file(path: impl AsRef<Path>) -> Result<ed25519::SecretKey> {
    let path = path.as_ref();
    let bytes = fs::read(path)
        .await
        .with_context(|| format!("No secret file at {}", path.display()))?;
    let secret_key = ed25519::SecretKey::from_bytes(bytes)?;

    Ok(secret_key)
}

async fn write_secret_key_to_file(secret_key: &ed25519::SecretKey, path: PathBuf) -> Result<()> {
    if let Some(parent) = path.parent() {
        DirBuilder::new()
            .recursive(true)
            .create(parent)
            .await
            .with_context(|| {
                format!(
                    "Could not create directory for secret file: {}",
                    parent.display()
                )
            })?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
        .with_context(|| format!("Could not generate secret file at {}", path.display()))?;

    file.write_all(secret_key.as_ref()).await?;

    Ok(())
}

fn create_swarm(identity: identity::Keypair) -> Result<Swarm<Behaviour>> {
    let local_peer_id = identity.public().into_peer_id();

    let transport = create_transport(&identity).context("Failed to create transport")?;
    let rendezvous = Rendezvous::new(identity, Config::default());
    let swarm = SwarmBuilder::new(transport, Behaviour::new(rendezvous), local_peer_id)
        .executor(Box::new(|f| {
            tokio::spawn(f);
        }))
        .build();

    Ok(swarm)
}

fn create_transport(identity: &identity::Keypair) -> Result<Boxed<(PeerId, StreamMuxerBox)>> {
    let auth_upgrade = {
        let noise_identity = noise::Keypair::<X25519Spec>::new().into_authentic(identity)?;
        NoiseConfig::xx(noise_identity).into_authenticated()
    };
    let multiplex_upgrade = SelectUpgrade::new(YamuxConfig::default(), MplexConfig::new());

    let transport = TokioDnsConfig::system(TokioTcpConfig::new().nodelay(true))
        .context("Failed to create DNS transport")?
        .upgrade(Version::V1)
        .authenticate(auth_upgrade)
        .multiplex(multiplex_upgrade)
        .timeout(Duration::from_secs(20))
        .map(|(peer, muxer), _| (peer, StreamMuxerBox::new(muxer)))
        .boxed();

    Ok(transport)
}

#[derive(Debug)]
enum Event {
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

#[derive(libp2p::NetworkBehaviour)]
#[behaviour(event_process = false)]
#[behaviour(out_event = "Event")]
struct Behaviour {
    ping: Ping,
    rendezvous: Rendezvous,
}

impl Behaviour {
    fn new(rendezvous: Rendezvous) -> Self {
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

struct Addresses<'a>(&'a [Multiaddr]);

// Prints an array of multiaddresses as a comma seperated string
impl fmt::Display for Addresses<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let display = self
            .0
            .iter()
            .map(|addr| addr.to_string())
            .collect::<Vec<String>>()
            .join(",");
        write!(f, "{}", display)
    }
}
