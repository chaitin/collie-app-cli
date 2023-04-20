use const_format::formatcp;

/// The rustc semver
pub const RUSTC_SEMVER: &str = env!("VERGEN_RUSTC_SEMVER");

/// Generate a timestamp string representing the build date (UTC).
pub const BUILD_DATE: &str = env!("VERGEN_BUILD_TIMESTAMP");

/// Short sha of the latest git commit.
pub const GIT_SHA: &str = env!("VERGEN_GIT_SHA");

/// Timestamp of the latest git commit.
pub const COMMIT_DATE: &str = env!("VERGEN_GIT_COMMIT_TIMESTAMP");

/// Collie engine crate version.
pub const SEMVER: &str = env!("CARGO_PKG_VERSION");

/// A ci task id.
pub const CI_TASK_ID: Option<&str> = option_env!("CI_TASK_ID");

pub const fn version() -> &'static str {
    formatcp!(
        r#"
Version:         {}
Rustc version:   {}
Git commit/date: {}/{}
Pipeline ID:     {}
Built:           {}
OS/Arch:         {}/{}"#,
        SEMVER,
        RUSTC_SEMVER,
        GIT_SHA,
        COMMIT_DATE,
        CI_TASK_ID.unwrap_or("self-build"),
        BUILD_DATE,
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

pub const fn short_version() -> &'static str {
    formatcp!("{SEMVER}+{GIT_SHA}")
}
