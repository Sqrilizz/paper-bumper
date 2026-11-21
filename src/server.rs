use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug)]
pub struct SmokeTestConfig {
    pub server_jar_path: PathBuf,
    pub plugin_jar_path: PathBuf,
    pub work_dir: PathBuf,
    pub timeout_seconds: u64,
    pub test_commands: Vec<String>,
    pub jvm_args: Vec<String>,
}

#[derive(Debug)]
pub struct SmokeTestReport {
    pub success: bool,
    pub summary_markdown: String,
}

pub struct SmokeTestRunner {
    config: SmokeTestConfig,
}

impl SmokeTestRunner {
    pub fn new(config: SmokeTestConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self) -> Result<SmokeTestReport> {
        self.prepare_workspace().await?;

        let report = self.execute_server().await?;
        Ok(report)
    }

    async fn prepare_workspace(&self) -> Result<()> {
        if self.config.work_dir.exists() {
            fs::remove_dir_all(&self.config.work_dir)
                .context("Failed to clean previous test-server directory")?;
        }
        fs::create_dir_all(&self.config.work_dir)
            .context("Failed to create test-server directory")?;

        let plugins_dir = self.config.work_dir.join("plugins");
        fs::create_dir_all(&plugins_dir).context("Failed to create plugins directory")?;

        let target_server_jar = self.config.work_dir.join("server.jar");
        fs::copy(&self.config.server_jar_path, &target_server_jar)
            .context("Failed to copy server JAR into test directory")?;

        let plugin_file_name = self
            .config
            .plugin_jar_path
            .file_name()
            .context("Invalid plugin JAR path")?;
        let target_plugin_jar = plugins_dir.join(plugin_file_name);
        fs::copy(&self.config.plugin_jar_path, &target_plugin_jar)
            .context("Failed to copy plugin JAR into plugins directory")?;

        let eula_file = self.config.work_dir.join("eula.txt");
        fs::write(eula_file, "eula=true\n").context("Failed to write eula.txt")?;

        let server_properties = self.config.work_dir.join("server.properties");
        let server_props_content = [
            "online-mode=false",
            "max-tick-time=-1",
            "difficulty=peaceful",
            "gamemode=creative",
            "level-type=minecraft:flat",
            "generate-structures=false",
            "view-distance=3",
            "simulation-distance=2",
            "sync-chunk-writes=false",
            "pause-when-empty-seconds=-1",
            "network-compression-threshold=-1",
            "white-list=false",
            "enforce-secure-profile=false",
        ]
        .join("\n");
        fs::write(server_properties, server_props_content)
            .context("Failed to write server.properties")?;

        Ok(())
    }

    async fn execute_server(&self) -> Result<SmokeTestReport> {
        let mut cmd = Command::new("java");
        for arg in &self.config.jvm_args {
            cmd.arg(arg);
        }
        cmd.arg("-jar")
            .arg("server.jar")
            .arg("--nogui")
            .current_dir(&self.config.work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .context("Failed to launch Minecraft server process")?;

        let mut stdin = child
            .stdin
            .take()
            .context("Failed to acquire process stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("Failed to acquire process stdout")?;
        let stderr = child
            .stderr
            .take()
            .context("Failed to acquire process stderr")?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut server_started = false;
        let mut plugin_enabled = false;
        let mut clean_shutdown = false;
        let mut collected_logs = Vec::new();
        let mut detected_errors = Vec::new();

        let total_timeout = Duration::from_secs(self.config.timeout_seconds);

        let test_execution = async {
            loop {
                tokio::select! {
                    line_result = stdout_reader.next_line() => {
                        match line_result {
                            Ok(Some(line)) => {
                                println!("{}", line);
                                collected_logs.push(line.clone());

                                if line.contains("Done (") {
                                    server_started = true;
                                    for command in &self.config.test_commands {
                                        let cmd_str = format!("{}\n", command);
                                        let _ = stdin.write_all(cmd_str.as_bytes()).await;
                                        let _ = stdin.flush().await;
                                        tokio::time::sleep(Duration::from_millis(1500)).await;
                                    }
                                    let _ = stdin.write_all(b"stop\n").await;
                                    let _ = stdin.flush().await;
                                }

                                if line.contains("Enabling ") {
                                    plugin_enabled = true;
                                }

                                if line.contains("Disabling ") {
                                    clean_shutdown = true;
                                }

                                if line.contains("[ERROR]")
                                    || line.contains("Exception in thread")
                                    || line.contains("Could not load 'plugins/")
                                    || line.contains("ClassNotFoundException")
                                    || line.contains("NoSuchMethodError")
                                    || line.contains("PluginClassLoader")
                                {
                                    detected_errors.push(line);
                                }
                            }
                            Ok(None) => break,
                            Err(e) => {
                                detected_errors.push(format!("Error reading server stdout: {}", e));
                                break;
                            }
                        }
                    }
                    err_line_result = stderr_reader.next_line() => {
                        match err_line_result {
                            Ok(Some(err_line)) => {
                                eprintln!("{}", err_line);
                                collected_logs.push(format!("[STDERR] {}", err_line));
                                detected_errors.push(err_line);
                            }
                            Ok(None) => {}
                            Err(e) => {
                                detected_errors.push(format!("Error reading server stderr: {}", e));
                            }
                        }
                    }
                }
            }
            let status = child.wait().await?;
            Ok::<std::process::ExitStatus, anyhow::Error>(status)
        };

        let execution_result = timeout(total_timeout, test_execution).await;

        let full_log_pathfull_log_path = self.config.work_dir.join("smoke_test_server.log");
        let full_log_text = collected_logs.join("\n");
        let _ = fs::write(&full_log_pathfull_log_path, &full_log_text);

        let final_status = match execution_result {
            Ok(Ok(status)) => status.success(),
            Ok(Err(err)) => {
                detected_errors.push(format!("Process error: {}", err));
                false
            }
            Err(_) => {
                detected_errors.push("Server execution timed out".to_string());
                false
            }
        };

        let overall_success =
            final_status && server_started && plugin_enabled && detected_errors.is_empty();

        let mut summary_markdown = String::new();
        summary_markdown.push_str("### Smoke Test Result\n\n");
        summary_markdown.push_str(&format!("- Server Started: `{}`\n", server_started));
        summary_markdown.push_str(&format!("- Plugin Enabled: `{}`\n", plugin_enabled));
        summary_markdown.push_str(&format!("- Clean Shutdown: `{}`\n", clean_shutdown));
        summary_markdown.push_str(&format!("- Exit Success: `{}`\n", final_status));
        summary_markdown.push_str(&format!("- Overall Success: `{}`\n\n", overall_success));

        if !detected_errors.is_empty() {
            summary_markdown.push_str("#### Detected Errors / Stacktraces\n\n```\n");
            for err in detected_errors.iter().take(20) {
                summary_markdown.push_str(err);
                summary_markdown.push('\n');
            }
            summary_markdown.push_str("```\n");
        }

        Ok(SmokeTestReport {
            success: overall_success,
            summary_markdown,
        })
    }
}
