use anyhow::{Context, Result};
use libp2p::core::identity::ed25519::Keypair;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::Boxed;
use libp2p::core::upgrade::{SelectUpgrade, Version};
use libp2p::dns::TokioDnsConfig;
use libp2p::futures::StreamExt;
use libp2p::identity::ed25519::SecretKey;
use libp2p::mplex::MplexConfig;
use libp2p::noise::{NoiseConfig, X25519Spec};
use libp2p::ping::{Ping, PingConfig, PingEvent};
use libp2p::rendezvous::{Config, Event as RendezvousEvent, Rendezvous};
use libp2p::swarm::{SwarmBuilder, SwarmEvent};
use libp2p::tcp::TokioTcpConfig;
use libp2p::yamux::YamuxConfig;
use libp2p::{identity, noise, rendezvous, Multiaddr, PeerId, Transport};
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
        long = "secret-file",
        help = "Path to the file that contains the secret key of the rendezvous server's identity keypair"
    )]
    secret_file: PathBuf,
    #[structopt(
        long,
        short,
        help = "Set this flag to generate a secret file at the path specified by the --secret-file argument"
    )]
    generate_secret: bool,
    #[structopt(long = "port")]
    port: u16,
    #[structopt(long = "json", help = "Format logs as JSON")]
    pub json: bool,
    #[structopt(long = "timestamp", help = "Include timestamp in logs")]
    pub timestamp: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::from_args();

    init_tracing(LevelFilter::INFO, cli.json, cli.timestamp);

    let secret_key = match cli.generate_secret {
        true => generate_secret_key_file(cli.secret_file).await?,
        false => load_secret_key_from_file(&cli.secret_file).await?,
    };

    let identity = identity::Keypair::Ed25519(secret_key.into());

    let transport = create_transport(&identity).context("Failed to create transport")?;

    let rendezvous = Rendezvous::new(identity.clone(), Config::default());

    let peer_id = PeerId::from(identity.public());

    let mut swarm = SwarmBuilder::new(transport, Behaviour::new(rendezvous), peer_id)
        .executor(Box::new(|f| {
            tokio::spawn(f);
        }))
        .build();

    tracing::info!(peer_id=%swarm.local_peer_id(), "Rendezvous server peer id");

    swarm
        .listen_on(
            format!("/ip4/0.0.0.0/tcp/{}", cli.port)
                .parse()
                .expect("static string is valid MultiAddress"),
        )
        .context("Failed to initialize listener")?;

    loop {
        let event = swarm.next().await;

        if let Some(event) = event {
            match event {
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

#[derive(libp2p::NetworkBehaviour)]
#[behaviour(event_process = false)]
#[behaviour(out_event = "Event")]
pub struct Behaviour {
    ping: Ping,
    rendezvous: Rendezvous,
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

pub async fn load_secret_key_from_file(path: impl AsRef<Path> + fmt::Debug) -> Result<SecretKey> {
    let bytes = fs::read(&path)
        .await
        .with_context(|| format!("No secret file at {:?}", path))?;
    let secret_key = SecretKey::from_bytes(bytes)?;
    Ok(secret_key)
}

pub async fn generate_secret_key_file(path: PathBuf) -> Result<SecretKey> {
    if let Some(parent) = path.parent() {
        DirBuilder::new()
            .recursive(true)
            .create(parent)
            .await
            .with_context(|| format!("Could not create directory for secret file: {:?}", parent))?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
        .with_context(|| format!("Could not generate secret file at {:?}", &path))?;

    let keypair = Keypair::generate();
    let secret_key = SecretKey::from(keypair);

    file.write_all(secret_key.as_ref()).await?;

    Ok(secret_key)
}

pub fn init_tracing(level: LevelFilter, json_format: bool, timestamp: bool) {
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

    if !timestamp {
        builder.without_time().init();
        return;
    }
    builder.init();
}

pub struct Addresses<'a>(pub &'a [Multiaddr]);

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

pub fn create_transport(identity: &identity::Keypair) -> Result<Boxed<(PeerId, StreamMuxerBox)>> {
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
