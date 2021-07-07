use anyhow::Result;
use libp2p::dns::TokioDnsConfig;
use libp2p::futures::StreamExt;
use libp2p::rendezvous::{Config, Namespace, Rendezvous};
use libp2p::swarm::{SwarmBuilder, SwarmEvent};
use libp2p::tcp::TokioTcpConfig;
use libp2p::{identity, rendezvous, Multiaddr, PeerId, Transport};
use rendezvous_server::transport::authenticate_and_multiplex;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Cli {
    #[structopt(long = "rendezvous-peer_id")]
    rendezvous_peer_id: PeerId,
    #[structopt(long = "rendezvous-addr")]
    rendezvous_addr: Multiaddr,
    #[structopt(long = "namespace")]
    namespace: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::from_args();

    let identity = identity::Keypair::generate_ed25519();

    let rendezvous_point_address = cli.rendezvous_addr;
    let rendezvous_point = cli.rendezvous_peer_id;

    let tcp_with_dns = TokioDnsConfig::system(TokioTcpConfig::new().nodelay(true)).unwrap();

    let transport = authenticate_and_multiplex(tcp_with_dns.boxed(), &identity).unwrap();

    let rendezvous = Rendezvous::new(identity.clone(), Config::default());

    let peer_id = PeerId::from(identity.public());

    let mut swarm = SwarmBuilder::new(transport, rendezvous, peer_id)
        .executor(Box::new(|f| {
            tokio::spawn(f);
        }))
        .build();

    println!("Local peer id: {}", swarm.local_peer_id());

    swarm.dial_addr(rendezvous_point_address.clone()).unwrap();

    while let Some(event) = swarm.next().await {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == rendezvous_point => {
                println!(
                    "Connected to rendezvous point, discovering nodes in '{}' namespace ...",
                    cli.namespace
                );

                swarm.behaviour_mut().discover(
                    Some(Namespace::new(cli.namespace.clone())?),
                    None,
                    None,
                    rendezvous_point,
                );
            }
            SwarmEvent::UnreachableAddr { error, address, .. }
            | SwarmEvent::UnknownPeerUnreachableAddr { error, address, .. }
                if address == rendezvous_point_address =>
            {
                println!(
                    "Failed to connect to rendezvous point at {}: {}",
                    address, error
                );
            }
            SwarmEvent::Behaviour(rendezvous::Event::Discovered { registrations, .. }) => {
                for registration in registrations {
                    for address in registration.record.addresses() {
                        let peer = registration.record.peer_id();
                        println!("Discovered peer {} at {}", peer, address);
                    }
                }
            }
            other => {
                println!("Unhandled {:?}", other);
            }
        }
    }

    Ok(())
}
