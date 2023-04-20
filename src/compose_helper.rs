use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use futures::{pin_mut, select, FutureExt};
use log::debug;
use regex::Regex;
use semver::{Version, VersionReq};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use which::which;

lazy_static::lazy_static! {
    static ref SERVER_REGEX: Regex = Regex::new(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?").unwrap();
    static ref COMPOSE_IN_DOCKER_VERSION: VersionReq = VersionReq::parse(">=20.10.13").unwrap();
}

pub(crate) async fn compose<T, I, S>(token: CancellationToken, target: T, args: I) -> Result<()>
where
    T: AsRef<Path>,
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    // run with docker compose
    #[cfg(target_family = "windows")]
    let docker_cli = which("docker.exe").context("can't find your docker.exe program")?;

    #[cfg(target_family = "unix")]
    let docker_cli = which("docker").context("can't find your docker program")?;

    let wait_for_cancel = token.cancelled().fuse();
    pin_mut!(wait_for_cancel);

    let command_fut = Command::new(&docker_cli)
        .args(["version", "--format", "'{{.Client.Version}}'"])
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .fuse();
    pin_mut!(command_fut);
    let docker_version_output = select! {
        _ = wait_for_cancel => return Ok(()),
        result = command_fut => result.context("check docker version")?
    };
    let docker_version = String::from_utf8_lossy(&docker_version_output.stdout);
    let Some(docker_version) = SERVER_REGEX.find(&docker_version) else {
        bail!("can't extract docker version from {docker_version}");
    };
    debug!("current docker version: {}", docker_version.as_str());
    let docker_version = Version::parse(docker_version.as_str()).unwrap();
    if COMPOSE_IN_DOCKER_VERSION.matches(&docker_version) {
        debug!("run with docker compose");

        let command_fut = Command::new(docker_cli)
            .arg("compose")
            .args(args)
            .current_dir(target)
            .kill_on_drop(true)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .fuse();
        pin_mut!(command_fut);

        select! {
            _ = wait_for_cancel => return Ok(()),
            result = command_fut => result?
        };
    } else {
        #[cfg(target_family = "windows")]
        let docker_compose_cli =
            which("docker-compose.exe").context("can't find your docker-compose.exe program")?;

        #[cfg(target_family = "unix")]
        let docker_compose_cli =
            which("docker-compose").context("can't find your docker-compose program")?;

        debug!("run with docker-compose");

        let command_fut = Command::new(docker_compose_cli)
            .args(args)
            .current_dir(target)
            .kill_on_drop(true)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .fuse();
        pin_mut!(command_fut);

        select! {
            _ = wait_for_cancel => return Ok(()),
            result = command_fut => result?
        };
    }
    Ok(())
}
