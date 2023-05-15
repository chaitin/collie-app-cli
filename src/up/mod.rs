mod rand_pass;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use async_recursion::async_recursion;
use futures::{pin_mut, select, FutureExt};
use handlebars::{no_escape, Handlebars};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use which::which;

use self::rand_pass::rand_pass_helper;
use crate::compose_helper::compose;
use crate::MANIFEST_FILENAME;

#[derive(Debug, Serialize, Deserialize)]
struct Metadata {
    name: String,
    desc: String,
    tags: Vec<String>,
    version: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Port {
    ip: String,
    port: u16,
    desc: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Variable {
    name: String,
    desc: String,
    value: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    metadata: Metadata,
    templates: Vec<String>,
    ports: HashMap<String, Port>,
    variables: HashMap<String, Variable>,
}

#[async_recursion]
async fn copy_dirs<S: AsRef<Path> + Send, D: AsRef<Path> + Send>(from: S, to: D) -> Result<()> {
    let from = from.as_ref().canonicalize().context("get from abs path")?;
    let to = to.as_ref().canonicalize().context("get to abs path")?;

    // Check if the source and destination are valid directories
    if !from.is_dir() || !to.is_dir() {
        bail!("source and destination must be directories");
    }

    // Iterate over the entries in the source directory
    let mut read_dir = fs::read_dir(from).await.context("read dir")?;
    while let Some(entry) = read_dir.next_entry().await.context("get dir next entry")? {
        let path = entry.path();

        // Get the file name of the entry
        let file_name = match path.file_name() {
            Some(name) => name,
            None => continue, // Skip if no file name
        };

        if path == to {
            continue; // Skip if file self ref
        }

        // Construct the new path by joining the destination and the file name
        let new_path = to.join(file_name);
        // Copy the entry to the new path
        if path.is_file() {
            if new_path.exists() {
                // Remove if is exists
                fs::remove_file(&new_path).await.context("remove file")?;
            }
            fs::copy(&path, &new_path).await.context("copy file")?;
        } else if path.is_dir() {
            if new_path.exists() {
                // Remove if is exists
                fs::remove_dir_all(&new_path).await.context("remove dir")?;
            }
            // If the entry is a directory, create a new directory and recursively copy its contents
            fs::create_dir_all(&new_path)
                .await
                .context("create target dir")?;
            copy_dirs(&path, &new_path).await?;
        }
    }
    Ok(())
}

pub(super) async fn render_and_up<P: AsRef<Path>, T: AsRef<Path>>(
    dir: P,
    target: T,
    dry: bool,
    token: CancellationToken,
) -> Result<()> {
    let dir = dir.as_ref();
    let target = target.as_ref();

    let wait_for_cancel = token.cancelled().fuse();
    pin_mut!(wait_for_cancel);

    // check if target exist and create it
    if !target.exists() {
        select! {
            _ = wait_for_cancel => return Ok(()),
            result = fs::create_dir_all(&target).fuse() => {
                result.with_context(|| format!("create target dir: {}", target.display()))?
            }
        }
    }

    copy_dirs(dir, target)
        .await
        .context("copy file to target dir")?;

    let manifest_file_path = dir.join(MANIFEST_FILENAME);
    let manifest_content = select! {
        _ = wait_for_cancel => return Ok(()),
        result = fs::read_to_string(&manifest_file_path).fuse() => {
            result.with_context(|| format!("read file: {}", manifest_file_path.display()))?
        }
    };
    let manifest: Manifest =
        serde_yaml::from_str(&manifest_content).context("can't parse config")?;

    // create the handlebars registry
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(no_escape);
    handlebars.set_strict_mode(true);
    // {{rand_pass <local_var_name> <pass_len>}}
    handlebars.register_helper("rand_pass", Box::new(rand_pass_helper));
    for template_rel_path in &manifest.templates {
        let template_file_path = target.join(template_rel_path);
        let template_content = fs::read_to_string(template_file_path)
            .await
            .context("read template content")?;
        handlebars
            .register_template_string(template_rel_path, &template_content)
            .context("compile template file")?;
    }

    for template_rel_path in &manifest.templates {
        let template_file_path = target.join(template_rel_path);
        let final_file_content = handlebars
            .render(template_rel_path, &manifest)
            .context("render template")?;
        fs::write(template_file_path, final_file_content)
            .await
            .context("write result to file")?;
    }
    if dry {
        return Ok(());
    }

    // run custom init script
    let init_script_file = target
        .join("scripts")
        .join("init.sh")
        .canonicalize()
        .context("init.sh path illegal")?;
    let shell = which("sh").context("can't find your sh")?;
    Command::new(shell)
        .args([OsStr::new("-c"), init_script_file.as_os_str()])
        .current_dir(target)
        .kill_on_drop(true)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .with_context(|| format!("run 'sh -c {}'", init_script_file.display()))?;

    // compose up the app
    compose(token.clone(), &target, ["up", "-d"])
        .await
        .context("run 'docker[.exe] compose up -d'")?;
    Ok(())
}
