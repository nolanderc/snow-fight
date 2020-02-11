//! Author(s):
//! - Christofer Nolander (cnol@kth.se)

use structopt::StructOpt;
use std::net::IpAddr;

#[derive(StructOpt)]
pub struct Options {
    /// The address of the server to connect to.
    #[structopt(short, long, default_value = "0.0.0.0")]
    pub addr: IpAddr,

    /// The port of the server to connect to.
    #[structopt(short, long, default_value = "8999")]
    pub port: u16,

    /// The verbosity level of the logger.
    #[structopt(long, default_value = "info")]
    pub log_level: log::LevelFilter,
}


