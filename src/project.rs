use anyhow::{bail, Context, Result};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

use crate::version::MinecraftVersion;

#[derive(Debug)]
pub struct ProjectMetadata {
    pub current_version: Option<MinecraftVersion>,
}

pub struct ProjectManager {
    root_dir: PathBuf,
}

impl ProjectManager {
    pub fn new(root_dir: impl AsRef<Path>) -> Self {
        Self {
            root_dir: root_dir.as_ref().to_path_buf(),
        }
    }

    pub fn inspect(&self) -> Result<ProjectMetadata> {
        let current_version = self.detect_version()?;
        Ok(ProjectMetadata { current_version })
    }

    fn detect_version(&self) -> Result<Option<MinecraftVersion>> {
        let gradle_props = self.root_dir.join("gradle.properties");
        if gradle_props.is_file() {
            let content =
                fs::read_to_string(&gradle_props).context("Failed to read gradle.properties")?;
            let re = Regex::new(r"(?m)^(minecraftVersion|minecraft_version|mcVersion|mc_version|paperVersion|paper_version)\s*=\s*([0-9a-zA-Z\.\-]+)")
                .expect("valid regex");
            for cap in re.captures_iter(&content) {
                if let Some(val) = cap.get(2) {
                    let raw = val.as_str().trim();
                    let cleaned = raw.trim_end_matches("-R0.1-SNAPSHOT");
                    return Ok(Some(MinecraftVersion::parse(cleaned)));
                }
            }
        }

        let re_paper = Regex::new(r"io\.papermc\.paper:paper-api:([0-9\.]+)").expect("valid regex");
        let re_userdev = Regex::new(r"paperweight-userdev:([0-9\.]+)").expect("valid regex");
        let re_paper_val =
            Regex::new(r#"(?:val|var)\s+paperApiVersion\s*=\s*"([0-9\.]+)"#).expect("valid regex");

        for gradle_file in ["build.gradle.kts", "build.gradle"] {
            let path = self.root_dir.join(gradle_file);
            if path.is_file() {
                let content = fs::read_to_string(&path).context("Failed to read build file")?;
                if let Some(cap) = re_paper.captures(&content) {
                    if let Some(m) = cap.get(1) {
                        return Ok(Some(MinecraftVersion::parse(m.as_str())));
                    }
                }
                if let Some(cap) = re_userdev.captures(&content) {
                    if let Some(m) = cap.get(1) {
                        return Ok(Some(MinecraftVersion::parse(m.as_str())));
                    }
                }
                if let Some(cap) = re_paper_val.captures(&content) {
                    if let Some(m) = cap.get(1) {
                        return Ok(Some(MinecraftVersion::parse(m.as_str())));
                    }
                }
            }
        }

        let pom_path = self.root_dir.join("pom.xml");
        if pom_path.is_file() {
            let content = fs::read_to_string(&pom_path).context("Failed to read pom.xml")?;
            let re_prop = Regex::new(r"<(?:minecraft\.version|paper\.version)>([0-9\.]+)</")
                .expect("valid regex");
            if let Some(cap) = re_prop.captures(&content) {
                if let Some(m) = cap.get(1) {
                    return Ok(Some(MinecraftVersion::parse(m.as_str())));
                }
            }
            let re_dep = Regex::new(r"<artifactId>paper-api</artifactId>\s*<version>([0-9\.]+)-R0\.1-SNAPSHOT</version>")
                .expect("valid regex");
            if let Some(cap) = re_dep.captures(&content) {
                if let Some(m) = cap.get(1) {
                    return Ok(Some(MinecraftVersion::parse(m.as_str())));
                }
            }
        }

        let re_api = Regex::new(r#"api-version:\s*['"]?([0-9\.]+)['"]?"#).expect("valid regex");
        for desc_name in ["paper-plugin.yml", "plugin.yml"] {
            for sub in ["src/main/resources", "."] {
                let desc_path = self.root_dir.join(sub).join(desc_name);
                if desc_path.is_file() {
                    let content = fs::read_to_string(&desc_path)
                        .context("Failed to read plugin descriptor")?;
                    if let Some(cap) = re_api.captures(&content) {
                        if let Some(m) = cap.get(1) {
                            return Ok(Some(MinecraftVersion::parse(m.as_str())));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    pub fn apply_version_update(&self, new_version: &MinecraftVersion) -> Result<bool> {
        let mut modified_any = false;
        let target_ver = &new_version.raw;
        let api_ver = new_version.compute_api_version();

        let gradle_props = self.root_dir.join("gradle.properties");
        if gradle_props.is_file() {
            let original = fs::read_to_string(&gradle_props)?;
            let mut updated = original.clone();
            let patterns = [
                (
                    r"(?m)^(minecraftVersion\s*=\s*).*$",
                    format!("${{1}}{}", target_ver),
                ),
                (
                    r"(?m)^(minecraft_version\s*=\s*).*$",
                    format!("${{1}}{}", target_ver),
                ),
                (
                    r"(?m)^(mcVersion\s*=\s*).*$",
                    format!("${{1}}{}", target_ver),
                ),
                (
                    r"(?m)^(mc_version\s*=\s*).*$",
                    format!("${{1}}{}", target_ver),
                ),
                (
                    r"(?m)^(paperVersion\s*=\s*).*$",
                    format!("${{1}}{}-R0.1-SNAPSHOT", target_ver),
                ),
                (
                    r"(?m)^(paper_version\s*=\s*).*$",
                    format!("${{1}}{}-R0.1-SNAPSHOT", target_ver),
                ),
            ];
            for (pat, repl) in patterns {
                let re = Regex::new(pat).expect("valid regex");
                updated = re.replace_all(&updated, repl.as_str()).to_string();
            }
            if updated != original {
                fs::write(&gradle_props, updated)?;
                modified_any = true;
            }
        }

        let re_paper = Regex::new(r"(io\.papermc\.paper:paper-api:)[0-9\.]+(-R0\.1-SNAPSHOT)?")
            .expect("valid regex");
        let re_userdev = Regex::new(r"(paperweight\.userdev:paperweight-userdev:)[0-9\.]+")
            .expect("valid regex");
        let re_paper_val_update =
            Regex::new(r#"((?:val|var)\s+paperApiVersion\s*=\s*")[0-9\.]+((\.build\.\+)?")"#)
                .expect("valid regex");

        for gradle_file in ["build.gradle.kts", "build.gradle"] {
            let path = self.root_dir.join(gradle_file);
            if path.is_file() {
                let original = fs::read_to_string(&path)?;
                let mut updated = original.clone();
                updated = re_paper
                    .replace_all(&updated, format!("${{1}}{}-R0.1-SNAPSHOT", target_ver))
                    .to_string();
                updated = re_userdev
                    .replace_all(&updated, format!("${{1}}{}", target_ver))
                    .to_string();
                updated = re_paper_val_update
                    .replace_all(&updated, format!("${{1}}{}${{2}}", target_ver))
                    .to_string();

                if updated != original {
                    fs::write(&path, updated)?;
                    modified_any = true;
                }
            }
        }

        let pom_path = self.root_dir.join("pom.xml");
        if pom_path.is_file() {
            let original = fs::read_to_string(&pom_path)?;
            let mut updated = original.clone();
            let re_prop = Regex::new(r"(<(?:minecraft\.version|paper\.version)>)[0-9\.]+(</(?:minecraft\.version|paper\.version)>)").expect("valid regex");
            updated = re_prop
                .replace_all(&updated, format!("${{1}}{}${{2}}", target_ver))
                .to_string();

            let re_dep = Regex::new(r"(<artifactId>paper-api</artifactId>\s*<version>)[0-9\.]+(-R0\.1-SNAPSHOT)?(</version>)").expect("valid regex");
            updated = re_dep
                .replace_all(
                    &updated,
                    format!("${{1}}{}-R0.1-SNAPSHOT${{3}}", target_ver),
                )
                .to_string();

            if updated != original {
                fs::write(&pom_path, updated)?;
                modified_any = true;
            }
        }

        let re_api_replace =
            Regex::new(r#"(api-version:\s*['"]?)[0-9\.]+(['"]?)"#).expect("valid regex");
        for desc_name in ["paper-plugin.yml", "plugin.yml"] {
            for sub in ["src/main/resources", "."] {
                let desc_path = self.root_dir.join(sub).join(desc_name);
                if desc_path.is_file() {
                    let original = fs::read_to_string(&desc_path)?;
                    let updated = re_api_replace
                        .replace_all(&original, format!("${{1}}{}${{2}}", api_ver))
                        .to_string();

                    if updated != original {
                        fs::write(&desc_path, updated)?;
                        modified_any = true;
                    }
                }
            }
        }

        Ok(modified_any)
    }

    pub async fn build_project(&self) -> Result<()> {
        let gradlew_path = self.root_dir.join("gradlew");
        let mvnw_path = self.root_dir.join("mvnw");
        let pom_path = self.root_dir.join("pom.xml");

        let (executable, args) = if gradlew_path.exists() {
            (
                gradlew_path.to_string_lossy().to_string(),
                vec!["build", "-x", "test"],
            )
        } else if mvnw_path.exists() {
            (
                mvnw_path.to_string_lossy().to_string(),
                vec!["package", "-DskipTests"],
            )
        } else if pom_path.exists() {
            ("mvn".to_string(), vec!["package", "-DskipTests"])
        } else {
            ("gradle".to_string(), vec!["build", "-x", "test"])
        };

        let mut child = Command::new(&executable)
            .args(&args)
            .current_dir(&self.root_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("Failed to spawn build process using {}", executable))?;

        let status = child
            .wait()
            .await
            .context("Build process failed during execution")?;
        if !status.success() {
            bail!(
                "Build command '{} {:?}' exited with status {}",
                executable,
                args,
                status
            );
        }

        Ok(())
    }

    pub fn find_plugin_jar(&self, explicit_path: Option<&str>) -> Result<PathBuf> {
        if let Some(explicit) = explicit_path {
            let candidate = self.root_dir.join(explicit);
            if candidate.is_file() {
                return Ok(candidate);
            }
            bail!(
                "Specified plugin jar path does not exist: {}",
                candidate.display()
            );
        }

        let mut candidates = Vec::new();
        let search_dirs = [
            self.root_dir.join("build").join("libs"),
            self.root_dir.join("target"),
        ];

        for dir in &search_dirs {
            if !dir.is_dir() {
                continue;
            }
            let entries = fs::read_dir(dir)
                .with_context(|| format!("Reading directory {}", dir.display()))?;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("jar") {
                    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    if file_name.ends_with("-sources.jar") || file_name.ends_with("-javadoc.jar") {
                        continue;
                    }
                    candidates.push(path);
                }
            }
        }

        if candidates.is_empty() {
            bail!(
                "Could not find any compiled plugin jar in build/libs or target. Run build first."
            );
        }

        if let Some(shadow_jar) = candidates.iter().find(|p| {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            name.ends_with("-all.jar")
        }) {
            return Ok(shadow_jar.clone());
        }

        Ok(candidates[0].clone())
    }
}
