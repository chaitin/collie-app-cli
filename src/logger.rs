use anyhow::Result;
use log::LevelFilter;
use log4rs::append::console::{ConsoleAppender, Target};
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::Config;

fn build_logger_config(level_filter: LevelFilter, stderr: bool) -> Result<Config> {
    let mut root_builder = Root::builder();
    let mut config_builder = Config::builder();
    let console_pattern =
        PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S %Z)(local)} - {h({l})} - [{M}] - {m}{n}");
    let console = ConsoleAppender::builder()
        .encoder(Box::new(console_pattern))
        .target(if stderr {
            Target::Stderr
        } else {
            Target::Stdout
        })
        .build();
    config_builder =
        config_builder.appender(Appender::builder().build("console", Box::new(console)));
    root_builder = root_builder.appender("console");
    Ok(config_builder.build(root_builder.build(level_filter))?)
}

pub fn setup_logger_by_verbose(verbose: u8, stderr: bool) -> Result<()> {
    let level = match verbose {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };
    let log4rs_config = match build_logger_config(level, stderr) {
        Ok(v) => v,
        Err(err) => {
            let err = err.context("can't setup logger");
            println!("{err:#?}");
            return Err(err);
        },
    };
    log4rs::init_config(log4rs_config)?;
    Ok(())
}
