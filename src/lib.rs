use anyhow::Context;
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};
use tokio::fs;

use hf_hub::HFClient;

pub mod agent;
pub mod audio_input;
pub mod client;
pub mod mode;
pub mod model_engines;
pub mod providers;
pub mod voice_agent;

pub struct Downloader {
    base_cache_path: PathBuf,
}

impl Downloader {
    pub fn new<P: AsRef<Path>>(base_cache_path: P) -> Self {
        Self {
            base_cache_path: base_cache_path.as_ref().to_path_buf(),
        }
    }

    pub async fn from_huggingface_path(&self, path: &str) -> anyhow::Result<PathBuf> {
        Ok(self
            .from_huggingface_paths(&[path])
            .await?
            .into_iter()
            .next()
            .unwrap())
    }

    pub async fn from_huggingface_paths(&self, paths: &[&str]) -> anyhow::Result<Vec<PathBuf>> {
        let client = HFClient::builder()
            .cache_dir(self.base_cache_path.join("./huggingface/"))
            .build()?;
        let mut cache_paths: Vec<PathBuf> = vec![];
        for path in paths {
            let (account, remainder) = path.split_once("/").context("path parsing error")?;
            let (repo, path) = remainder.split_once("/").context("path parsing error")?;

            let repo = client.model(account, repo);
            cache_paths.push(repo.download_file().filename(path).send().await?);
        }

        Ok(cache_paths)
    }

    pub async fn from_github_path(&self, path: &str) -> anyhow::Result<PathBuf> {
        Ok(self
            .from_github_paths(&[path])
            .await?
            .into_iter()
            .next()
            .unwrap())
    }

    pub async fn from_github_paths(&self, paths: &[&str]) -> anyhow::Result<Vec<PathBuf>> {
        let mut github_urls: Vec<reqwest::Url> = vec![];

        for path in paths {
            let (account, remainder) = path.split_once("/").context("path parsing error")?;
            let (repo, path) = remainder.split_once("/").context("path parsing error")?;

            github_urls.push(reqwest::Url::from_str(
                format!("https://github.com/{account}/{repo}/raw/refs/heads/main/{path}").as_str(),
            )?);
        }

        self.from_urls(&github_urls).await
    }

    pub async fn from_url<T: reqwest::IntoUrl>(&self, url: T) -> anyhow::Result<PathBuf> {
        Ok(self.from_urls(&[url]).await?.into_iter().next().unwrap())
    }

    pub async fn from_urls<T: reqwest::IntoUrl>(&self, urls: &[T]) -> anyhow::Result<Vec<PathBuf>> {
        let mut cache_paths: Vec<PathBuf> = vec![];

        for url in urls {
            let general_url_file = reqwest::Url::from_str(url.as_str())?;
            let general_cache_path = &self
                .base_cache_path
                .join(general_url_file.host_str().unwrap_or("default"));
            if !general_cache_path.exists() {
                fs::create_dir_all(general_cache_path).await?;
            }
            let response = reqwest::get(general_url_file.as_ref())
                .await?
                .error_for_status()?;

            let cache_path = general_cache_path.join(&general_url_file.path()[1..]);
            if let Some(parent) = cache_path.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent).await?;
            }
            if !cache_path.exists() {
                fs::write(&cache_path, response.bytes().await?).await?;
            }
            cache_paths.push(cache_path);
        }

        Ok(cache_paths)
    }
}
