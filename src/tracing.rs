use libp2p::Multiaddr;
use std::fmt;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::FmtSubscriber;

pub fn init(level: LevelFilter, json_format: bool, timestamp: bool) {
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
