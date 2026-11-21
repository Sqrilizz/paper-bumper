mod api;
mod project;
mod server;
mod version;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::api::{ApiClient, ReleaseChannel, ServerSoftware};
use crate::project::ProjectManager;
use crate::server::{SmokeTestConfig, SmokeTestRunner};
use crate::version::MinecraftVersion;

#[derive(Parser)]
#[command(
    name = "paper-bumper",
    version = "0.1.0",
    about = "Automated Paper/Purpur plugin version updater and server smoke test runner"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Check {
        #[arg(short, long, default_value = ".")]
        project_dir: PathBuf,

        #[arg(short, long, default_value = "paper")]
        software: String,

        #[arg(short, long, default_value = "STABLE")]
        channel: String,
    },
    Bump {
        #[arg(short, long, default_value = ".")]
        project_dir: PathBuf,

        #[arg(short, long, default_value = "paper")]
        software: String,

        #[arg(short, long, default_value = "STABLE")]
        channel: String,

        #[arg(short, long)]
        target_version: Option<String>,
    },
    SmokeTest {
        #[arg(short, long)]
        server_jar: PathBuf,

        #[arg(short, long)]
        plugin_jar: PathBuf,

        #[arg(short, long, default_value = "test-server")]
        work_dir: PathBuf,

        #[arg(short, long, default_value = "120")]
        timeout: u64,

        #[arg(short, long, value_delimiter = ',')]
        commands: Vec<String>,
    },
    Run {
        #[arg(short, long, default_value = ".")]
        project_dir: PathBuf,

        #[arg(short, long, default_value = "paper")]
        software: String,

        #[arg(short, long, default_value = "STABLE")]
        channel: String,

        #[arg(short, long)]
        target_version: Option<String>,

        #[arg(long)]
        skip_build: bool,

        #[arg(long)]
        skip_smoke: bool,

        #[arg(long)]
        plugin_jar: Option<String>,

        #[arg(short, long, default_value = "test-server")]
        work_dir: PathBuf,

        #[arg(short, long, default_value = "120")]
        timeout: u64,

        #[arg(short, long, value_delimiter = ',')]
        commands: Vec<String>,
    },
}

fn write_github_output(key: &str, value: &str) {
    if let Ok(path) = env::var("GITHUB_OUTPUT") {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(file, "{}={}", key, value);
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check {
            project_dir,
            software,
            channel,
        } => {
            let server_software = ServerSoftware::parse(&software)?;
            let release_channel = ReleaseChannel::parse(&channel)?;
            let api = ApiClient::new()?;
            let project = ProjectManager::new(&project_dir);

            let metadata = project.inspect()?;
            let current = metadata.current_version;

            let versions = api
                .get_available_versions(server_software, release_channel)
                .await?;
            let latest = versions
                .last()
                .context("No available versions returned from server API")?;

            let has_update = match &current {
                Some(curr) => latest > curr,
                None => true,
            };

            let build = api
                .get_build_info(server_software, &latest.raw, release_channel)
                .await?;

            write_github_output("has_update", if has_update { "true" } else { "false" });
            write_github_output(
                "current_version",
                current
                    .as_ref()
                    .map(|v| v.raw.as_str())
                    .unwrap_or("unknown"),
            );
            write_github_output("target_version", &latest.raw);
            write_github_output("build_id", &build.build_id);
            write_github_output("download_url", &build.download_url);
            write_github_output("jar_name", &build.jar_name);

            println!(
                "Current version: {}",
                current.map(|v| v.raw).unwrap_or_else(|| "unknown".into())
            );
            println!("Target version:  {} (Build {})", latest.raw, build.build_id);
            println!("Has update:      {}", has_update);
        }

        Commands::Bump {
            project_dir,
            software,
            channel,
            target_version,
        } => {
            let server_software = ServerSoftware::parse(&software)?;
            let release_channel = ReleaseChannel::parse(&channel)?;
            let api = ApiClient::new()?;
            let project = ProjectManager::new(&project_dir);

            let target = match target_version {
                Some(tv) => MinecraftVersion::parse(&tv),
                None => {
                    let versions = api
                        .get_available_versions(server_software, release_channel)
                        .await?;
                    versions
                        .last()
                        .context("No versions returned from API")?
                        .clone()
                }
            };

            let changed = project.apply_version_update(&target)?;
            println!(
                "Updated files for version {}: changed={}",
                target.raw, changed
            );
        }

        Commands::SmokeTest {
            server_jar,
            plugin_jar,
            work_dir,
            timeout,
            commands,
        } => {
            let test_commands = if commands.is_empty() {
                vec!["version".to_string(), "plugins".to_string()]
            } else {
                commands
            };

            let config = SmokeTestConfig {
                server_jar_path: server_jar,
                plugin_jar_path: plugin_jar,
                work_dir,
                timeout_seconds: timeout,
                test_commands,
                jvm_args: vec![
                    "-Xms1G".to_string(),
                    "-Xmx2G".to_string(),
                    "-XX:+UseG1GC".to_string(),
                ],
            };

            let runner = SmokeTestRunner::new(config);
            let report = runner.run().await?;

            println!("{}", report.summary_markdown);
            write_github_output(
                "smoke_test_success",
                if report.success { "true" } else { "false" },
            );

            if !report.success {
                anyhow::bail!("Smoke test failed");
            }
        }

        Commands::Run {
            project_dir,
            software,
            channel,
            target_version,
            skip_build,
            skip_smoke,
            plugin_jar,
            work_dir,
            timeout,
            commands,
        } => {
            let server_software = ServerSoftware::parse(&software)?;
            let release_channel = ReleaseChannel::parse(&channel)?;
            let api = ApiClient::new()?;
            let project = ProjectManager::new(&project_dir);

            let metadata = project.inspect()?;
            let current = metadata.current_version;

            let target = match target_version {
                Some(tv) => MinecraftVersion::parse(&tv),
                None => {
                    let versions = api
                        .get_available_versions(server_software, release_channel)
                        .await?;
                    versions
                        .last()
                        .context("No versions returned from API")?
                        .clone()
                }
            };

            let has_update = match &current {
                Some(curr) => &target > curr,
                None => true,
            };

            let build = api
                .get_build_info(server_software, &target.raw, release_channel)
                .await?;

            write_github_output("has_update", if has_update { "true" } else { "false" });
            write_github_output(
                "current_version",
                current
                    .as_ref()
                    .map(|v| v.raw.as_str())
                    .unwrap_or("unknown"),
            );
            write_github_output("target_version", &target.raw);
            write_github_output("build_id", &build.build_id);
            write_github_output("download_url", &build.download_url);
            write_github_output("jar_name", &build.jar_name);

            println!(
                "Current version: {}",
                current.map(|v| v.raw).unwrap_or_else(|| "unknown".into())
            );
            println!("Target version:  {} (Build {})", target.raw, build.build_id);
            println!("Has update:      {}", has_update);

            if !has_update {
                println!("Project is already up to date.");
                return Ok(());
            }

            println!("Applying version update to project files...");
            project.apply_version_update(&target)?;

            if !skip_build {
                println!("Compiling plugin...");
                project.build_project().await?;
            }

            if !skip_smoke {
                let resolved_plugin_jar = project.find_plugin_jar(plugin_jar.as_deref())?;

                let cache_dir = env::temp_dir().join("paper-bumper-cache");
                tokio::fs::create_dir_all(&cache_dir).await?;
                let server_jar_dest = cache_dir.join(&build.jar_name);

                if !server_jar_dest.exists() {
                    println!("Downloading server jar {}...", build.jar_name);
                    api.download_file(&build.download_url, &server_jar_dest)
                        .await?;
                } else {
                    println!("Using cached server jar {}.", build.jar_name);
                }

                let test_commands = if commands.is_empty() {
                    vec!["version".to_string(), "plugins".to_string()]
                } else {
                    commands
                };

                let config = SmokeTestConfig {
                    server_jar_path: server_jar_dest,
                    plugin_jar_path: resolved_plugin_jar,
                    work_dir,
                    timeout_seconds: timeout,
                    test_commands,
                    jvm_args: vec![
                        "-Xms1G".to_string(),
                        "-Xmx2G".to_string(),
                        "-XX:+UseG1GC".to_string(),
                    ],
                };

                let runner = SmokeTestRunner::new(config);
                let report = runner.run().await?;

                println!("{}", report.summary_markdown);
                write_github_output(
                    "smoke_test_success",
                    if report.success { "true" } else { "false" },
                );

                if !report.success {
                    anyhow::bail!("Smoke test failed");
                }
            }
        }
    }

    Ok(())
}
