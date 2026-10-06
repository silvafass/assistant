use std::path::Path;

use anyhow::Context;
use hf_hub::HFClient;
use tokio::time::Instant;

use crate::{
    audio_input,
    model_engines::{WakeWordModel, WithRingBuffer},
};

const MINIMAL_PREDICTION_SCORE: f32 = 0.7;

pub struct LivekitWakeWordModel {
    inner_model: livekit_wakeword::WakeWordModel,
}

impl WithRingBuffer for LivekitWakeWordModel {}

impl LivekitWakeWordModel {
    pub fn new<P: AsRef<Path>>(paths: &[P]) -> anyhow::Result<Self> {
        let model = livekit_wakeword::WakeWordModel::new(paths, audio_input::AUDIO_RATE)?;

        Ok(LivekitWakeWordModel { inner_model: model })
    }

    pub async fn from_huggingface_paths(paths: &[&str]) -> anyhow::Result<Self> {
        let client = HFClient::new()?;
        let mut model_paths = vec![];
        for path in paths {
            let (account, remainder) = path.split_once("/").context("path parsing error")?;
            let (repo, path) = remainder.split_once("/").context("path parsing error")?;

            let repo = client.model(account, repo);
            model_paths.push(repo.download_file().filename(path).send().await?);
        }

        LivekitWakeWordModel::new(model_paths.as_slice())
    }
}

impl WakeWordModel for LivekitWakeWordModel {
    fn verify(&mut self, chunk: &[f32]) -> anyhow::Result<bool> {
        let chunk: Vec<i16> = chunk
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect();

        let start = Instant::now();
        let predictions = self.inner_model.predict(&chunk)?;
        dbg!(start.elapsed());

        Ok(predictions
            .iter()
            .find(|(_, score)| **score >= MINIMAL_PREDICTION_SCORE)
            .is_some())
    }
}
