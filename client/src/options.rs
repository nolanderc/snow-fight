//! Author(s):
//! - Christofer Nolander (cnol@kth.se)

use std::net::IpAddr;
use std::str::FromStr;

use structopt::StructOpt;

#[derive(StructOpt)]
pub struct Options {
    /// The address of the server to connect to.
    #[structopt(short, long, default_value = "0.0.0.0")]
    pub addr: IpAddr,

    /// The port of the server to connect to.
    #[structopt(short, long, default_value = "8999")]
    pub port: u16,

    /// The verbosity level of the logger.
    #[structopt(long, default_value = "warn")]
    pub log_level: Vec<LogFilter>,
}

#[derive(Debug, Clone)]
pub struct LogFilter {
    pub module: Option<String>,
    pub level: log::LevelFilter,
}

impl FromStr for LogFilter {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match *s.split(':').collect::<Vec<_>>().as_slice() {
            [level] => Ok(LogFilter {
                module: None,
                level: level.parse()?,
            }),
            [module, level] => Ok(LogFilter {
                module: Some(module.to_owned()),
                level: level.parse()?,
            }),
            _ => Err(anyhow!(
                "expected a level filter of the form `<level>` or `<module>:<level>`"
            )),
        }
    }
}
