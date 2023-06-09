use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use futures::{pin_mut, select, FutureExt};
use handlebars::Handlebars;
use rust_embed::RustEmbed;
use serde::Serialize;
use tokio::fs;
use tokio_util::sync::CancellationToken;

#[cfg(target_family = "unix")]
use crate::{INIT_SCRIPT_PATH, UNINSTALL_SCRIPT_PATH, UPGRADE_SCRIPT_PATH};

#[derive(Debug, Serialize)]
struct ManifestVar {
    app_id: String,
}

#[derive(RustEmbed)]
#[folder = "src/new/tpl"]
struct Tpl;

pub(super) async fn generate<P: AsRef<Path>>(
    dir: P,
    name: String,
    token: CancellationToken,
) -> Result<PathBuf> {
    let wait_for_cancel = token.cancelled().fuse();
    pin_mut!(wait_for_cancel);

    let path = dir.as_ref().join(&name);
    if path.exists() {
        bail!("{name} has been exist");
    }
    for file in Tpl::iter() {
        let file_path = path.join(&*file);
        if let Some(path) = file_path.parent() {
            if !path.exists() {
                select! {
                    _ = wait_for_cancel => break,
                    result = fs::create_dir_all(&path).fuse() => {
                        result.with_context(|| format!("create dir: {}", path.display()))?
                    }
                }
            };
        }
        let file_content = Tpl::get(&file).unwrap();
        select! {
            _ = wait_for_cancel => break,
            result = fs::write(&file_path, &file_content.data).fuse() => {
                result.with_context(|| format!("write file: {}", file_path.display()))?;
            }
        }

        #[cfg(target_family = "unix")]
        if file_path.ends_with(&*INIT_SCRIPT_PATH)
            || file_path.ends_with(&*UNINSTALL_SCRIPT_PATH)
            || file_path.ends_with(&*UPGRADE_SCRIPT_PATH)
        {
            use std::os::unix::fs::PermissionsExt;

            let mut perms = fs::metadata(&file_path)
                .await
                .with_context(|| format!("get {} meta", file_path.display()))?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&file_path, perms)
                .await
                .with_context(|| format!("set {} permissions", file_path.display()))?
        }
    }

    // render tpl manifest
    let manifest_path = path.join("manifest.yaml");
    let mut handlebars = Handlebars::new();
    let template_content = fs::read_to_string(&manifest_path)
        .await
        .context("read template content")?;
    handlebars
        .register_template_string("manifest.yaml", &template_content)
        .context("compile template file")?;
    let manifest_var = ManifestVar {
        app_id: format!("{}@COLI", xid::new()),
    };
    let manifest_file_content = handlebars
        .render("manifest.yaml", &manifest_var)
        .context("render template")?;
    fs::write(manifest_path, manifest_file_content)
        .await
        .context("write result to file")?;
    Ok(path)
}
