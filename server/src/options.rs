//! Author(s):
//! - Christofer Nolander (cnol@kth.se)

use structopt::StructOpt;
use std::net::IpAddr;

// Define some options that can be configured with command line arguments.
#[derive(StructOpt)]
pub struct Options {
    /// The ip addres to listen for incoming connections on.
    #[structopt(short, long, default_value = "0.0.0.0")]
    pub addr: IpAddr,

    /// The port to listen for incoming connections on.
    #[structopt(short, long, default_value = "8999")]
    pub port: u16,

    /// The verbosity of the logging.
    #[structopt(long, default_value = "info")]
    pub log_level: log::LevelFilter,
}


