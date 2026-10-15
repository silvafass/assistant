use std::{collections::HashMap, path::Path};

use anyhow::Ok;
use ndarray::{Array, Array1, Array2, Axis};
use ort::value::Tensor;
use tokio::time::Instant;

use crate::model_engines::{WakeWordModel, WithRingBuffer};

const MINIMAL_PREDICTION_SCORE: f32 = 0.5;

const EMBEDDING_DIM: usize = 96;
const EMBEDDING_WINDOW: usize = 76;
const EMBEDDING_STRIDE: usize = 8;
const MIN_EMBEDDINGS: usize = 16;

pub struct LivekitWakeWordModel {
    embedding_session: ort::session::Session,
    melspectrogram_session: ort::session::Session,
    classifier_sessions: HashMap<String, ort::session::Session>,
}

impl WithRingBuffer for LivekitWakeWordModel {}

impl LivekitWakeWordModel {
    pub fn new<P: AsRef<Path>>(
        melspectrogram_model_path: P,
        embedding_model_path: P,
        classifier_models_paths: &[P],
    ) -> anyhow::Result<Self> {
        let mut classifier_sessions = HashMap::new();
        for classifier_model_path in classifier_models_paths {
            classifier_sessions.insert(
                classifier_model_path
                    .as_ref()
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
                ort::session::Session::builder()?.commit_from_file(classifier_model_path)?,
            );
        }

        Ok(Self {
            melspectrogram_session: ort::session::Session::builder()?
                .commit_from_file(melspectrogram_model_path)?,
            embedding_session: ort::session::Session::builder()?
                .commit_from_file(embedding_model_path)?,
            classifier_sessions,
        })
    }

    fn run_melspectrogram_model(&mut self, chunk: &[f32]) -> anyhow::Result<Array2<f32>> {
        let chunk_array = Array1::from_vec(chunk.to_vec());

        let chunk_2d_array = chunk_array.insert_axis(Axis(0));
        let audio_tensor = Tensor::from_array(chunk_2d_array)?;

        let features = self
            .melspectrogram_session
            .run(ort::inputs![audio_tensor])?;

        let raw = features["output"].try_extract_array::<f32>()?;
        let rows = raw.shape()[2];
        let cols = raw.shape()[3];
        let mut output = raw.into_owned().into_shape_with_order((rows, cols))?;

        output.mapv_inplace(|x| x / 10.0 + 2.0);

        Ok(output)
    }

    fn run_embedding_model(&mut self, mel_features: &[f32]) -> anyhow::Result<Array1<f32>> {
        let input = Array::from_shape_vec((1, 76, 32, 1), mel_features.to_vec())?;
        let tensor = Tensor::from_array(input)?;

        let outputs = self.embedding_session.run(ort::inputs![tensor])?;

        let raw = outputs["conv2d_19"].try_extract_array::<f32>()?;
        let embedding = raw.into_owned().into_shape_with_order(EMBEDDING_DIM)?;

        Ok(embedding)
    }
}

impl WakeWordModel for LivekitWakeWordModel {
    fn verify(&mut self, chunk: &[f32]) -> anyhow::Result<bool> {
        let instant_start = Instant::now();

        let mel = self.run_melspectrogram_model(chunk)?;
        let num_frames = mel.shape()[0];

        if num_frames < EMBEDDING_WINDOW {
            return Ok(false);
        }

        let mut embeddings = Vec::new();
        let mut start = 0;
        while start + EMBEDDING_WINDOW <= num_frames {
            let window = mel.slice(ndarray::s![start..start + EMBEDDING_WINDOW, ..]);
            let window_slice = window.as_standard_layout();
            let emb = self.run_embedding_model(window_slice.as_slice().unwrap())?;
            embeddings.push(emb);
            start += EMBEDDING_STRIDE;
        }

        if embeddings.len() < MIN_EMBEDDINGS {
            return Ok(false);
        }

        let last = &embeddings[embeddings.len() - MIN_EMBEDDINGS..];
        let views: Vec<_> = last.iter().map(|e| e.view()).collect();
        let emb_sequence = ndarray::stack(Axis(0), &views)?;
        let emb_input = emb_sequence.insert_axis(Axis(0));

        let mut predictions = HashMap::new();
        for (name, session) in &mut self.classifier_sessions {
            let tensor = Tensor::from_array(emb_input.clone())?;
            let outputs = session.run(ort::inputs!["embeddings" => tensor])?;
            let raw = outputs["score"].try_extract_array::<f32>()?;
            let score = raw.iter().copied().next().unwrap_or(0.0);
            predictions.insert(name.clone(), score);
        }

        dbg!(instant_start.elapsed());

        Ok(predictions
            .iter()
            .find(|(_, score)| **score >= MINIMAL_PREDICTION_SCORE)
            .is_some())
    }
}
