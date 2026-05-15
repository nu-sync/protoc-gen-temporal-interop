use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use temporalio_sdk_core::ephemeral_server::{TemporalDevServerConfig, default_cached_download};
use tokio::process::{Child, Command};
use tokio::time::timeout;

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Install tools and regenerate Rust/TypeScript code.
    Gen,
    /// Run the full TypeScript-client to Rust-worker interop test.
    Test,
}

#[derive(Debug, Clone)]
struct Pins {
    ts_temporal_version: String,
    rust_temporal_repository: String,
    rust_temporal_ref: String,
}

#[derive(Debug, Clone)]
struct Paths {
    root: PathBuf,
    logs: PathBuf,
    tools: PathBuf,
    tool_bin: PathBuf,
    cargo_bin: PathBuf,
    rust_checkout: PathBuf,
    cargo_patch_config: PathBuf,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,interop_harness=debug".into()),
        )
        .try_init();

    let cli = Cli::parse();
    let paths = Paths::new(std::env::current_dir().context("read current directory")?);
    fs::create_dir_all(&paths.logs).context("create .dev-logs")?;
    fs::create_dir_all(&paths.tool_bin).context("create tool bin directory")?;

    match cli.command {
        Commands::Gen => run_gen(&paths).await,
        Commands::Test => run_test(&paths).await,
    }
}

impl Paths {
    fn new(root: PathBuf) -> Self {
        let tools = root.join(".dev-tools");
        Self {
            logs: root.join(".dev-logs"),
            tool_bin: tools.join("bin"),
            cargo_bin: tools.join("cargo/bin"),
            rust_checkout: root.join(".dev-rust"),
            cargo_patch_config: tools.join("cargo-patch.toml"),
            tools,
            root,
        }
    }
}

async fn run_gen(paths: &Paths) -> Result<()> {
    let pins = load_pins(&paths.root)?;
    let tool_path = ensure_tools(paths, &pins).await?;

    run_logged(
        paths,
        "npm-ci",
        "npm",
        ["--prefix", "ts-client", "ci", "--min-release-age=0"],
        &paths.root,
        &[],
        Duration::from_secs(120),
    )
    .await?;

    run_logged(
        paths,
        "buf-dep-update",
        "buf",
        ["dep", "update"],
        &paths.root,
        &[("PATH", tool_path.clone())],
        Duration::from_secs(60),
    )
    .await?;

    run_logged(
        paths,
        "buf-generate",
        "buf",
        ["generate"],
        &paths.root,
        &[("PATH", tool_path)],
        Duration::from_secs(60),
    )
    .await?;

    Ok(())
}

async fn run_test(paths: &Paths) -> Result<()> {
    run_gen(paths).await?;
    let _lockfile_guard = LockfileGuard::new(paths.root.join("Cargo.lock"))?;

    run_logged(
        paths,
        "cargo-check-interop-proto",
        "cargo",
        cargo_args(paths, ["check", "-p", "interop-proto"]),
        &paths.root,
        &[],
        Duration::from_secs(120),
    )
    .await?;
    run_logged(
        paths,
        "cargo-check-interop-worker",
        "cargo",
        cargo_args(paths, ["check", "-p", "interop-worker"]),
        &paths.root,
        &[],
        Duration::from_secs(120),
    )
    .await?;
    run_logged(
        paths,
        "npm-typecheck",
        "npm",
        ["--prefix", "ts-client", "run", "typecheck"],
        &paths.root,
        &[],
        Duration::from_secs(120),
    )
    .await?;
    run_logged(
        paths,
        "cargo-build-interop-worker",
        "cargo",
        cargo_args(paths, ["build", "-p", "interop-worker"]),
        &paths.root,
        &[],
        Duration::from_secs(120),
    )
    .await?;

    run_runtime(paths).await
}

async fn run_runtime(paths: &Paths) -> Result<()> {
    let server_stdout = File::create(paths.logs.join("temporal-server.log"))
        .context("create Temporal server log")?;
    let server_stderr = server_stdout
        .try_clone()
        .context("clone Temporal server log handle")?;
    let server_config = TemporalDevServerConfig::builder()
        .exe(default_cached_download())
        .ui(false)
        .build();
    let mut server = timeout(
        Duration::from_secs(60),
        server_config
            .start_server_with_output(Stdio::from(server_stdout), Stdio::from(server_stderr)),
    )
    .await
    .context("Temporal dev server start timed out after 60 seconds")?
    .context("start Temporal dev server")?;
    let target_address = server.target.to_string();

    let mut worker = spawn_logged(
        paths,
        "worker",
        "cargo",
        cargo_args(
            paths,
            [
                "run",
                "-p",
                "interop-worker",
                "--",
                "--target-address",
                target_address.as_str(),
                "--namespace",
                "default",
            ],
        ),
        &paths.root,
        &[],
    )
    .await?;

    let case_id = format!("case-{}", std::process::id());
    let customer_id = format!("customer-{case_id}");
    let cli_result = run_logged(
        paths,
        "ts-cli",
        "npm",
        [
            "--prefix",
            "ts-client",
            "run",
            "cli",
            "--",
            "run",
            "--target-address",
            &target_address,
            "--namespace",
            "default",
            "--case-id",
            &case_id,
            "--customer-id",
            &customer_id,
            "--finish-reason",
            "ci-finish",
        ],
        &paths.root,
        &[],
        Duration::from_secs(45),
    )
    .await;

    let worker_shutdown = shutdown_child(&mut worker, Duration::from_secs(10)).await;
    let server_shutdown = server.shutdown().await.context("shut down Temporal server");

    cli_result?;
    worker_shutdown?;
    server_shutdown?;
    Ok(())
}

fn load_pins(root: &Path) -> Result<Pins> {
    let path = root.join("pins/versions.env");
    let contents = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let mut map = BTreeMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .with_context(|| format!("invalid pins line: {line}"))?;
        map.insert(key.to_string(), value.to_string());
    }

    Ok(Pins {
        ts_temporal_version: std::env::var("TS_TEMPORAL_VERSION")
            .ok()
            .or_else(|| map.get("TS_TEMPORAL_VERSION").cloned())
            .context("TS_TEMPORAL_VERSION is missing")?,
        rust_temporal_repository: map
            .get("RUST_TEMPORAL_REPOSITORY")
            .cloned()
            .context("RUST_TEMPORAL_REPOSITORY is missing")?,
        rust_temporal_ref: map
            .get("RUST_TEMPORAL_REF")
            .cloned()
            .context("RUST_TEMPORAL_REF is missing")?,
    })
}

async fn ensure_tools(paths: &Paths, pins: &Pins) -> Result<OsString> {
    let rust_workspace = ensure_rust_workspace(paths, pins).await?;
    write_cargo_patch_config(paths, &rust_workspace)?;
    ensure_rust_plugin(paths, &rust_workspace).await?;
    ensure_ts_plugin(paths, pins).await?;
    ensure_prost_plugin(paths).await?;

    let mut entries = vec![
        paths.tool_bin.clone(),
        paths.cargo_bin.clone(),
        paths.root.join("ts-client/node_modules/.bin"),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        entries.extend(std::env::split_paths(&path));
    }
    std::env::join_paths(entries).context("join PATH entries")
}

async fn ensure_rust_workspace(paths: &Paths, pins: &Pins) -> Result<PathBuf> {
    if let Ok(workspace) = std::env::var("RUST_TEMPORAL_WORKSPACE") {
        PathBuf::from(workspace)
    } else {
        ensure_rust_checkout(paths, pins).await?;
        paths.rust_checkout.clone()
    }
    .canonicalize()
    .context("resolve Rust Temporal workspace")
}

async fn ensure_rust_plugin(paths: &Paths, workspace: &Path) -> Result<()> {
    if let Ok(plugin) = std::env::var("RUST_TEMPORAL_PLUGIN") {
        copy_tool(&plugin, &paths.tool_bin.join("protoc-gen-rust-temporal"))?;
        return Ok(());
    }

    run_logged(
        paths,
        "build-rust-plugin",
        "cargo",
        ["build", "-p", "protoc-gen-rust-temporal"],
        workspace,
        &[],
        Duration::from_secs(120),
    )
    .await?;
    copy_tool(
        workspace.join("target/debug/protoc-gen-rust-temporal"),
        &paths.tool_bin.join("protoc-gen-rust-temporal"),
    )
}

async fn ensure_rust_checkout(paths: &Paths, pins: &Pins) -> Result<()> {
    if !paths.rust_checkout.exists() {
        run_logged(
            paths,
            "clone-rust-temporal",
            "git",
            [
                "clone",
                &pins.rust_temporal_repository,
                path_str(&paths.rust_checkout)?,
            ],
            &paths.root,
            &[],
            Duration::from_secs(120),
        )
        .await?;
    }

    run_logged(
        paths,
        "checkout-rust-temporal",
        "git",
        ["checkout", &pins.rust_temporal_ref],
        &paths.rust_checkout,
        &[],
        Duration::from_secs(60),
    )
    .await
}

async fn ensure_ts_plugin(paths: &Paths, pins: &Pins) -> Result<()> {
    if let Ok(plugin) = std::env::var("TS_TEMPORAL_PLUGIN") {
        copy_tool(&plugin, &paths.tool_bin.join("protoc-gen-ts-temporal"))?;
        return Ok(());
    }

    let workspace = if let Ok(source) = std::env::var("TS_TEMPORAL_SOURCE") {
        PathBuf::from(source)
    } else {
        ensure_ts_checkout(paths, pins).await?
    };

    run_logged(
        paths,
        "build-ts-plugin",
        "cargo",
        ["build", "--release", "-p", "protoc-gen-ts-temporal"],
        &workspace,
        &[],
        Duration::from_secs(120),
    )
    .await?;
    copy_tool(
        workspace.join("target/release/protoc-gen-ts-temporal"),
        &paths.tool_bin.join("protoc-gen-ts-temporal"),
    )
}

async fn ensure_ts_checkout(paths: &Paths, pins: &Pins) -> Result<PathBuf> {
    let checkout = paths.tools.join(format!(
        "protoc-gen-ts-temporal-{}",
        pins.ts_temporal_version
    ));
    if !checkout.exists() {
        run_logged(
            paths,
            "clone-ts-temporal",
            "git",
            [
                "clone",
                "https://github.com/nu-sync/protoc-gen-ts-temporal",
                path_str(&checkout)?,
            ],
            &paths.root,
            &[],
            Duration::from_secs(120),
        )
        .await?;
    }

    let tag = format!("v{}", pins.ts_temporal_version);
    run_logged(
        paths,
        "checkout-ts-temporal",
        "git",
        ["checkout", &tag],
        &checkout,
        &[],
        Duration::from_secs(60),
    )
    .await?;
    Ok(checkout)
}

fn write_cargo_patch_config(paths: &Paths, rust_workspace: &Path) -> Result<()> {
    let runtime_path = rust_workspace.join("crates/temporal-proto-runtime");
    let bridge_path = rust_workspace.join("crates/temporal-proto-runtime-bridge");
    ensure!(
        runtime_path.join("Cargo.toml").is_file(),
        "missing temporal-proto-runtime crate at {}",
        runtime_path.display()
    );
    ensure!(
        bridge_path.join("Cargo.toml").is_file(),
        "missing temporal-proto-runtime-bridge crate at {}",
        bridge_path.display()
    );

    let config_dir = paths
        .cargo_patch_config
        .parent()
        .context("Cargo patch config path has no parent directory")?;
    fs::create_dir_all(config_dir).context("create Cargo patch config directory")?;
    let contents = format!(
        "[patch.\"https://github.com/nu-sync/protoc-gen-rust-temporal\"]\n\
         temporal-proto-runtime = {{ path = \"{}\" }}\n\
         temporal-proto-runtime-bridge = {{ path = \"{}\" }}\n",
        toml_path(&runtime_path)?,
        toml_path(&bridge_path)?
    );
    fs::write(&paths.cargo_patch_config, contents)
        .with_context(|| format!("write {}", paths.cargo_patch_config.display()))
}

struct LockfileGuard {
    path: PathBuf,
    original: Option<Vec<u8>>,
}

impl LockfileGuard {
    fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let original = match fs::read(&path) {
            Ok(contents) => Some(contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error).with_context(|| format!("read {}", path.display()));
            }
        };
        Ok(Self { path, original })
    }
}

impl Drop for LockfileGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(contents) => {
                let _ = fs::write(&self.path, contents);
            }
            None => {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

async fn ensure_prost_plugin(paths: &Paths) -> Result<()> {
    if which("protoc-gen-prost").is_some() {
        return Ok(());
    }

    run_logged(
        paths,
        "install-protoc-gen-prost",
        "cargo",
        [
            "install",
            "--root",
            path_str(&paths.tools.join("cargo"))?,
            "--locked",
            "protoc-gen-prost",
            "--version",
            "0.5.0",
        ],
        &paths.root,
        &[],
        Duration::from_secs(120),
    )
    .await
}

fn cargo_args<I, S>(paths: &Paths, args: I) -> Vec<OsString>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut all_args = vec![
        OsString::from("--config"),
        paths.cargo_patch_config.as_os_str().to_os_string(),
    ];
    all_args.extend(args.into_iter().map(|arg| arg.as_ref().to_os_string()));
    all_args
}

async fn run_logged<I, S>(
    paths: &Paths,
    name: &str,
    program: &str,
    args: I,
    cwd: &Path,
    envs: &[(&str, OsString)],
    limit: Duration,
) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut child = command_with_log(paths, name, program, args, cwd, envs)
        .with_context(|| format!("spawn {name}"))?;
    let status = wait_for_child(&mut child, name, limit).await?;
    ensure!(
        status.success(),
        "{name} failed with {status}; see {}",
        paths.logs.join(format!("{name}.log")).display()
    );
    Ok(())
}

async fn spawn_logged<I, S>(
    paths: &Paths,
    name: &str,
    program: &str,
    args: I,
    cwd: &Path,
    envs: &[(&str, OsString)],
) -> Result<Child>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    command_with_log(paths, name, program, args, cwd, envs).with_context(|| format!("spawn {name}"))
}

fn command_with_log<I, S>(
    paths: &Paths,
    name: &str,
    program: &str,
    args: I,
    cwd: &Path,
    envs: &[(&str, OsString)],
) -> Result<Child>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<_>>();
    let log_path = paths.logs.join(format!("{name}.log"));
    let mut stdout =
        File::create(&log_path).with_context(|| format!("create {}", log_path.display()))?;
    writeln!(stdout, "$ {}", format_command(program, &args))
        .with_context(|| format!("write command header to {}", log_path.display()))?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("clone {}", log_path.display()))?;

    let mut command = Command::new(program);
    command
        .args(&args)
        .current_dir(cwd)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    for (key, value) in envs {
        command.env(key, value);
    }
    command.spawn().with_context(|| format!("run {program}"))
}

async fn wait_for_child(child: &mut Child, name: &str, limit: Duration) -> Result<ExitStatus> {
    match timeout(limit, child.wait()).await {
        Ok(status) => status.with_context(|| format!("wait for {name}")),
        Err(_) => {
            let _ = child.start_kill();
            let _ = timeout(Duration::from_secs(5), child.wait()).await;
            bail!("{name} timed out after {} seconds", limit.as_secs())
        }
    }
}

async fn shutdown_child(child: &mut Child, limit: Duration) -> Result<()> {
    match child.try_wait().context("check child status")? {
        Some(status) if status.success() => return Ok(()),
        Some(status) => bail!("worker exited early with {status}"),
        None => {}
    }

    child.kill().await.context("stop worker")?;
    let status = timeout(limit, child.wait())
        .await
        .context("worker did not exit after kill")?
        .context("wait for worker shutdown")?;
    ensure!(
        status.success() || status.signal().is_some(),
        "worker exited with {status}"
    );
    Ok(())
}

fn copy_tool(from: impl AsRef<Path>, to: &Path) -> Result<()> {
    let from = from.as_ref();
    ensure!(from.exists(), "tool does not exist: {}", from.display());
    if to.exists() {
        fs::remove_file(to).with_context(|| format!("remove {}", to.display()))?;
    }
    fs::copy(from, to).with_context(|| format!("copy {} to {}", from.display(), to.display()))?;
    Ok(())
}

fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn format_command(program: &str, args: &[OsString]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().map(|arg| {
            let arg = arg.to_string_lossy();
            if arg.chars().any(char::is_whitespace) {
                format!("{:?}", arg.as_ref())
            } else {
                arg.into_owned()
            }
        }))
        .collect::<Vec<_>>()
        .join(" ")
}

fn toml_path(path: &Path) -> Result<String> {
    Ok(path_str(path)?.replace('\\', "\\\\"))
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}

#[cfg(unix)]
trait ExitStatusExt {
    fn signal(&self) -> Option<i32>;
}

#[cfg(unix)]
impl ExitStatusExt for std::process::ExitStatus {
    fn signal(&self) -> Option<i32> {
        std::os::unix::process::ExitStatusExt::signal(self)
    }
}

#[cfg(not(unix))]
trait ExitStatusExt {
    fn signal(&self) -> Option<i32>;
}

#[cfg(not(unix))]
impl ExitStatusExt for std::process::ExitStatus {
    fn signal(&self) -> Option<i32> {
        None
    }
}
