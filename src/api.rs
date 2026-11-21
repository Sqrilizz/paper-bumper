use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::version::MinecraftVersion;

const APP_USER_AGENT: &str = "paper-bumper/0.1.0 (https://github.com/example/paper-bumper)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerSoftware {
    Paper,
    Purpur,
}

impl ServerSoftware {
    pub fn parse(input: &str) -> Result<Self> {
        match input.to_lowercase().as_str() {
            "paper" => Ok(Self::Paper),
            "purpur" => Ok(Self::Purpur),
            other => bail!("Unsupported server software: {}", other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseChannel {
    Stable,
    Experimental,
}

impl ReleaseChannel {
    pub fn parse(input: &str) -> Result<Self> {
        match input.to_uppercase().as_str() {
            "STABLE" => Ok(Self::Stable),
            "EXPERIMENTAL" => Ok(Self::Experimental),
            other => bail!("Unsupported release channel: {}", other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerBuild {
    pub build_id: String,
    pub download_url: String,
    pub jar_name: String,
}

#[derive(Deserialize)]
struct PaperProjectResponse {
    versions: HashMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct PaperBuildDownloadItem {
    url: String,
    name: Option<String>,
}

#[derive(Deserialize)]
struct PaperBuildEntry {
    id: u64,
    channel: Option<String>,
    downloads: Option<HashMap<String, PaperBuildDownloadItem>>,
}

#[derive(Deserialize)]
struct PurpurProjectResponse {
    versions: Vec<String>,
}

#[derive(Deserialize)]
struct PurpurBuildsSection {
    latest: Option<String>,
}

#[derive(Deserialize)]
struct PurpurVersionResponse {
    builds: Option<PurpurBuildsSection>,
}

pub struct ApiClient {
    client: reqwest::Client,
}

impl ApiClient {
    pub fn new() -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(APP_USER_AGENT));
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to construct reqwest client")?;
        Ok(Self { client })
    }

    pub async fn get_available_versions(
        &self,
        software: ServerSoftware,
        channel: ReleaseChannel,
    ) -> Result<Vec<MinecraftVersion>> {
        match software {
            ServerSoftware::Paper => self.get_paper_versions(channel).await,
            ServerSoftware::Purpur => self.get_purpur_versions().await,
        }
    }

    async fn get_paper_versions(&self, channel: ReleaseChannel) -> Result<Vec<MinecraftVersion>> {
        let url = "https://fill.papermc.io/v3/projects/paper";
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to query Paper project API")?;

        if !resp.status().is_success() {
            bail!("Paper project API returned status: {}", resp.status());
        }

        let body: PaperProjectResponse = resp
            .json()
            .await
            .context("Failed to parse Paper project response")?;

        let mut versions = Vec::new();
        for group in body.versions.values() {
            for v_str in group {
                let parsed = MinecraftVersion::parse(v_str);
                if channel == ReleaseChannel::Stable && !parsed.is_stable {
                    continue;
                }
                versions.push(parsed);
            }
        }

        versions.sort();
        Ok(versions)
    }

    async fn get_purpur_versions(&self) -> Result<Vec<MinecraftVersion>> {
        let url = "https://api.purpurmc.org/v2/purpur";
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to query Purpur project API")?;

        if !resp.status().is_success() {
            bail!("Purpur project API returned status: {}", resp.status());
        }

        let body: PurpurProjectResponse = resp
            .json()
            .await
            .context("Failed to parse Purpur project response")?;

        let mut versions: Vec<MinecraftVersion> = body
            .versions
            .into_iter()
            .map(|v| MinecraftVersion::parse(&v))
            .collect();

        versions.sort();
        Ok(versions)
    }

    pub async fn get_build_info(
        &self,
        software: ServerSoftware,
        version: &str,
        channel: ReleaseChannel,
    ) -> Result<ServerBuild> {
        match software {
            ServerSoftware::Paper => self.get_paper_build(version, channel).await,
            ServerSoftware::Purpur => self.get_purpur_build(version).await,
        }
    }

    async fn get_paper_build(&self, version: &str, channel: ReleaseChannel) -> Result<ServerBuild> {
        let url = format!(
            "https://fill.papermc.io/v3/projects/paper/versions/{}/builds",
            version
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to query Paper builds API")?;

        if !resp.status().is_success() {
            bail!(
                "Paper builds API returned status: {} for version {}",
                resp.status(),
                version
            );
        }

        let builds: Vec<PaperBuildEntry> = resp
            .json()
            .await
            .context("Failed to parse Paper builds response")?;

        if builds.is_empty() {
            bail!("No builds available for Paper version {}", version);
        }

        let target_build = if channel == ReleaseChannel::Stable {
            builds
                .iter()
                .find(|b| b.channel.as_deref() == Some("STABLE"))
                .or_else(|| builds.first())
        } else {
            builds.first()
        };

        let selected = target_build.context("Could not find a valid build")?;
        let downloads = selected
            .downloads
            .as_ref()
            .context("Build missing downloads section")?;
        let server_download = downloads
            .get("server:default")
            .context("Build missing server:default download key")?;

        let jar_name = server_download
            .name
            .clone()
            .unwrap_or_else(|| format!("paper-{}-{}.jar", version, selected.id));

        Ok(ServerBuild {
            build_id: selected.id.to_string(),
            download_url: server_download.url.clone(),
            jar_name,
        })
    }

    async fn get_purpur_build(&self, version: &str) -> Result<ServerBuild> {
        let url = format!("https://api.purpurmc.org/v2/purpur/{}", version);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to query Purpur version API")?;

        if !resp.status().is_success() {
            bail!(
                "Purpur version API returned status: {} for version {}",
                resp.status(),
                version
            );
        }

        let data: PurpurVersionResponse = resp
            .json()
            .await
            .context("Failed to parse Purpur version response")?;

        let latest_build = data
            .builds
            .and_then(|b| b.latest)
            .context("No builds found in Purpur version response")?;

        let download_url = format!(
            "https://api.purpurmc.org/v2/purpur/{}/{}/download",
            version, latest_build
        );
        let jar_name = format!("purpur-{}-{}.jar", version, latest_build);

        Ok(ServerBuild {
            build_id: latest_build,
            download_url,
            jar_name,
        })
    }

    pub async fn download_file(&self, url: &str, destination: &Path) -> Result<()> {
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create parent directory for download")?;
        }

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to initiate file download")?;

        if !resp.status().is_success() {
            bail!("File download failed with status: {}", resp.status());
        }

        let mut stream = resp.bytes_stream();
        let mut file = File::create(destination)
            .await
            .context("Failed to create destination file on disk")?;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Error reading download stream chunk")?;
            file.write_all(&chunk)
                .await
                .context("Error writing chunk to destination file")?;
        }

        file.flush()
            .await
            .context("Failed to flush downloaded file")?;
        Ok(())
    }
}
