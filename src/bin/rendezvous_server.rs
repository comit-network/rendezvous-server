use anyhow::Result;
use libp2p::dns::TokioDnsConfig;
use libp2p::futures::StreamExt;
use libp2p::rendezvous::{Config, Event as RendezvousEvent, Rendezvous};
use libp2p::swarm::{SwarmBuilder, SwarmEvent};
use libp2p::tcp::TokioTcpConfig;
use libp2p::{identity, PeerId, Transport};
use rendezvous_server::tracing::init;
use rendezvous_server::transport::authenticate_and_multiplex;
use rendezvous_server::{generate_secret_key_file, load_secret_key_from_file, Behaviour, Event};
use std::path::PathBuf;
use structopt::StructOpt;
use tracing::level_filters::LevelFilter;

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
}

#[tokio::main]
async fn main() -> Result<()> {
    init(LevelFilter::INFO, true)?;
    let cli = Cli::from_args();

    let secret_key = match cli.generate_secret {
        true => generate_secret_key_file(cli.secret_file).await?,
        false => load_secret_key_from_file(&cli.secret_file).await?,
    };

    let identity = identity::Keypair::Ed25519(secret_key.into());

    let tcp_with_dns = TokioDnsConfig::system(TokioTcpConfig::new().nodelay(true)).unwrap();

    let transport = authenticate_and_multiplex(tcp_with_dns.boxed(), &identity).unwrap();

    let rendezvous = Rendezvous::new(identity.clone(), Config::default());

    let peer_id = PeerId::from(identity.public());

    let mut swarm = SwarmBuilder::new(transport, Behaviour::new(rendezvous), peer_id)
        .executor(Box::new(|f| {
            tokio::spawn(f);
        }))
        .build();

    tracing::info!(peer_id=%swarm.local_peer_id(), "Rendezvous server peer id");

    swarm
        .listen_on(format!("/ip4/0.0.0.0/tcp/{}", cli.port).parse().unwrap())
        .unwrap();

    loop {
        let event = swarm.next().await;

        if let Some(event) = event {
            match event {
                SwarmEvent::Behaviour(Event::Rendezvous(RendezvousEvent::PeerRegistered {
                    peer,
                    registration,
                })) => {
                    tracing::info!(%peer, namespace=%registration.namespace, addresses=?registration.record.addresses(), ttl=%registration.ttl,  "Peer Registered");
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
                    tracing::info!(peer=%registration.record.peer_id(), namespace=%registration.namespace, addresses=?registration.record.addresses(), ttl=%registration.ttl, "Registration expired");
                }
                SwarmEvent::Behaviour(Event::Rendezvous(RendezvousEvent::PeerUnregistered {
                    peer,
                    namespace,
                })) => {
                    tracing::info!(%peer, %namespace, "Peer unregistered");
                }
                _ => {}
            }
        }
    }
}
