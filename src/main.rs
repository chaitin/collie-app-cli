#![feature(const_option_ext)]
#![feature(path_file_prefix)]

mod compose_helper;
mod down;
mod logger;
mod new;
mod up;
mod version;

use std::env::{self, current_exe};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use futures::future::Fuse;
use futures::{pin_mut, select, FutureExt};
use log::{debug, error, info, warn};
use logger::setup_logger_by_verbose;
use tokio::signal;
use tokio_util::sync::CancellationToken;

use self::version::{short_version, version};

lazy_static::lazy_static! {
    static ref CURRENT_FILENAME: String = current_exe().unwrap().file_name().unwrap().to_string_lossy().to_string();
    pub(crate) static ref INIT_SCRIPT_PATH: PathBuf = Path::new("scripts").join("init.sh");
    pub(crate) static ref UNINSTALL_SCRIPT_PATH: PathBuf = Path::new("scripts").join("uninstall.sh");
    pub(crate) static ref UPGRADE_SCRIPT_PATH: PathBuf = Path::new("scripts").join("upgrade.sh");
}

pub(crate) const MANIFEST_FILENAME: &str = "manifest.yaml";

const SHORT_HEADER: &str = r#"
╔═╗╔═╗╦  ╦  ╦╔═╗
║  ║ ║║  ║  ║║╣ 
╚═╝╚═╝╩═╝╩═╝╩╚═╝"#;
const LONG_HEADER: &str = r#"
╔═╗╔═╗╦  ╦  ╦╔═╗
║  ║ ║║  ║  ║║╣ 
╚═╝╚═╝╩═╝╩═╝╩╚═╝
Make you develop Collie APP easy!"#;

#[derive(Parser)]
#[command(
    name = "collie-app-cli",
    version = short_version(),
    long_version = version(),
    author = "Chaitin Collie Agent Team",
    before_help = SHORT_HEADER,
    before_long_help = LONG_HEADER,
)]
struct Opts {
    /// Verbose (-v, -vv, -vvv, etc.)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Workspace dir
    #[arg(short, long, global = true, default_value_os_t = env::current_dir().unwrap_or(env::temp_dir()))]
    dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a empty APP
    New {
        /// The App name
        name: String,
    },
    /// Up like docker compose up
    Up {
        /// Where is the APP render to default is .render
        #[arg(short, long, default_value_os_t = PathBuf::from(".render"))]
        target_dir: PathBuf,
        /// Just see the result not really run
        #[arg(long)]
        dry: bool,
    },
    /// Down like docker compose down
    Down {
        /// Where is the APP render to default is .render
        #[arg(short, long, default_value_os_t = PathBuf::from(".render"))]
        target_dir: PathBuf,
    },
}

async fn deliver_command<P: AsRef<Path>>(
    dir: P,
    command: Command,
    token: CancellationToken,
) -> ExitCode {
    match command {
        Command::New {
            name,
        } => match new::generate(dir, name, token).await {
            Ok(final_path) => {
                println!("Create success.");
                println!("~~~~~~~ TRY ~~~~~~~");
                println!("cd {}", final_path.display());
                println!("{} run", &*CURRENT_FILENAME);
                println!("~~~~~~~~~~~~~~~~~~~");
                ExitCode::SUCCESS
            },
            Err(err) => {
                error!("new application: {err:#}");
                ExitCode::FAILURE
            },
        },
        Command::Up {
            target_dir,
            dry,
        } => match up::render_and_up(dir, target_dir, dry, token).await {
            Ok(()) => {
                println!("Up success.");
                ExitCode::SUCCESS
            },
            Err(err) => {
                error!("Up app: {err:#}");
                ExitCode::FAILURE
            },
        },
        Command::Down {
            target_dir,
        } => match down::down(target_dir, token).await {
            Ok(()) => {
                println!("Down success.");
                ExitCode::SUCCESS
            },
            Err(err) => {
                error!("Down app: {err:#}");
                ExitCode::FAILURE
            },
        },
    }
}

async fn real_main() -> ExitCode {
    // parse from cmdline
    let opts: Opts = Opts::parse();

    // setup logger
    if let Err(err) = setup_logger_by_verbose(opts.verbose, true) {
        eprintln!("can't setup logger: {err:#}");
        return ExitCode::FAILURE;
    };

    // exec command
    let token = CancellationToken::new();

    let real_task = deliver_command(opts.dir, opts.command, token.clone()).fuse();
    pin_mut!(real_task);

    let exit_sig = async move {
        #[cfg(target_family = "unix")]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate())?;
            select! {
                _ = sigterm.recv().fuse() => {},
                v = signal::ctrl_c().fuse() => v?,
            };
            return Ok(());
        }
        #[cfg(not(target_family = "unix"))]
        {
            signal::ctrl_c().await?;
            return Ok(());
        }
        #[allow(unreachable_code)]
        Result::<_>::Ok(())
    }
    .fuse();
    pin_mut!(exit_sig);

    let run_shutdown = Fuse::terminated();
    pin_mut!(run_shutdown);

    let graceful_shutdown_timeout = Fuse::terminated();
    pin_mut!(graceful_shutdown_timeout);

    loop {
        select! {
            result = exit_sig => {
                match result {
                    Ok(()) => {
                        info!("receive signal interrupt -> exec graceful shutdown");
                        run_shutdown.set(token.cancelled().fuse());
                        token.cancel();
                    },
                    Err(err) => {
                        error!("unable to listen for shutdown signal: {err:#}");
                        break ExitCode::FAILURE;
                    },
                }
            },
            _ = run_shutdown => {
                let timeout = tokio::time::sleep(Duration::from_secs(30)).fuse();
                graceful_shutdown_timeout.set(timeout);
            },
            _ = graceful_shutdown_timeout => {
                warn!("graceful shutdown timeout");
                break ExitCode::from(0xeb); // 0xeb > exit bad
            },
            exit_code = &mut real_task => {
                debug!("graceful shutdown");
                break exit_code;
            },
        }
    }
}

fn main() -> ExitCode {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .thread_stack_size(1024 * 1024)
        .thread_name("tokio worker")
        .build()
        .unwrap()
        .block_on(real_main())
}
