use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use which::which;

use crate::compose_helper::compose;

pub(super) async fn down<T: AsRef<Path>>(target: T, token: CancellationToken) -> Result<()> {
    let target = target.as_ref();

    // check if target exist and create it
    if !target.exists() {
        bail!("targe not exist");
    }

    compose(token.clone(), &target, ["down"])
        .await
        .context("run 'docker[.exe] compose down'")?;

    // run custom uninstall script
    let uninstall_script_file = target
        .join("scripts")
        .join("uninstall.sh")
        .canonicalize()
        .context("uninstall.sh path illegal")?;
    let shell = which("sh").context("can't find your sh")?;
    Command::new(shell)
        .args([OsStr::new("-c"), uninstall_script_file.as_os_str()])
        .current_dir(target)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .with_context(|| format!("run 'sh -c {}'", uninstall_script_file.display()))?;
    Ok(())
}
