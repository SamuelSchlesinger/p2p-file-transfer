use futures::future;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId, Swarm};
use log::{info, warn};
use std::path::Path;
use std::task::Poll;
use structopt::StructOpt;

#[derive(StructOpt)]
struct Options {
    #[structopt(
        short = "i",
        long = "identity",
        about = "The file containing the user's cryptographic identity",
        default_value = "p2p.id"
    )]
    keypair: String,
    #[structopt(
        short = "p",
        long = "peers",
        about = "A file containing a list of peer addresses",
        default_value = "p2p.peers"
    )]
    peers: String,
    #[structopt(
        short = "l",
        short = "listen",
        about = "The multiaddr to listen on",
        default_value = "/ip4/0.0.0.0/tcp/0"
    )]
    listen: Multiaddr,
}

#[derive(Debug)]
enum Error {
    IO(std::io::Error),
    Decoding(libp2p::identity::error::DecodingError),
    Multiaddr(libp2p::multiaddr::Error),
    Transport(libp2p::TransportError<std::io::Error>),
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Error {
        Error::IO(error)
    }
}

impl From<libp2p::multiaddr::Error> for Error {
    fn from(error: libp2p::multiaddr::Error) -> Error {
        Error::Multiaddr(error)
    }
}

impl From<libp2p::TransportError<std::io::Error>> for Error {
    fn from(error: libp2p::TransportError<std::io::Error>) -> Error {
        Error::Transport(error)
    }
}

impl From<libp2p::identity::error::DecodingError> for Error {
    fn from(error: libp2p::identity::error::DecodingError) -> Error {
        Error::Decoding(error)
    }
}

fn keypair(options: &Options) -> Result<Keypair, Error> {
    let keypair_path = Path::new(&options.keypair);
    if !keypair_path.exists() {
        use std::io::Write;
        let new_keypair = libp2p::identity::ed25519::Keypair::generate();
        std::fs::File::create(keypair_path)?.write(&new_keypair.encode())?;
        Ok(Keypair::Ed25519(new_keypair))
    } else {
        use std::io::Read;
        let mut keypair = [0; 64];
        std::fs::File::open(keypair_path)?.read(&mut keypair)?;
        Ok(Keypair::Ed25519(
            libp2p::identity::ed25519::Keypair::decode(&mut keypair)?,
        ))
    }
}

fn peers(options: &Options) -> Result<Vec<Multiaddr>, Error> {
    let peers_path = Path::new(&options.peers);
    if peers_path.exists() {
        use std::io::Read;
        let mut peers = String::new();
        std::fs::File::open(peers_path)?.read_to_string(&mut peers)?;
        Ok(peers.lines().filter_map(|line| line.parse().ok()).collect())
    } else {
        Ok(Vec::new())
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    let options = Options::from_args();
    let keypair = keypair(&options)?;
    let peer_id = PeerId::from(keypair.public());

    info!("My peer ID: {:?}", peer_id);

    let peers = peers(&options)?;

    info!("Connecting to peers: {:?}", peers);

    let transport = libp2p::development_transport(keypair).await?;

    let behavior = libp2p::ping::Ping::new(libp2p::ping::PingConfig::new().with_keep_alive(true));

    let mut swarm = Swarm::new(transport, behavior, peer_id);

    swarm.listen_on(options.listen)?;

    for addr in peers {
        match swarm.dial(addr.clone()) {
            Ok(()) => info!("Connected to {:?}", addr),
            Err(e) => warn!("Couldn't connect to {:?} with error {:?}", addr, e),
        }
    }

    future::poll_fn(move |cx| loop {
        match swarm.poll_next_unpin(cx) {
            Poll::Ready(Some(event)) => info!("Swarm event: {:?}", event),
            Poll::Ready(None) => return Poll::Ready(()),
            Poll::Pending => return Poll::Pending,
        }
    })
    .await;

    Ok(())
}
