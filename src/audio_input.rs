use anyhow::Context;
use cpal::Stream;
use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;
use std::sync::mpsc::{self, Receiver};

pub const AUDIO_RATE: u32 = 16_000;
const MINIMAL_BUFFER_AMPLITUDE: f32 = 0.05;
const MAXIMUL_BUFFER_AMPLITUDE: f32 = 0.90;

pub struct AudioInputStream {
    receiver: Receiver<Vec<f32>>,
    stream: Stream,
}

impl AudioInputStream {
    pub fn next_chunk(&self) -> anyhow::Result<Vec<f32>> {
        self.receiver.recv().context("Error while getting chunk")
    }

    pub fn listen(&self, max_buffer: usize) -> Option<Vec<f32>> {
        let mut buffer: Vec<f32> = Vec::new();
        let mut max_buffer_amplitude = 0f32;
        while let Ok(chunk) = self.next_chunk() {
            buffer.extend_from_slice(&chunk);

            const CHUNK_SIZE: usize = (AUDIO_RATE * 3) as usize;
            if buffer.len() < CHUNK_SIZE {
                continue;
            }
            let max_chunk_amplitude = buffer
                .last_chunk::<CHUNK_SIZE>()
                .unwrap_or(&[0f32; CHUNK_SIZE])
                .iter()
                .take(CHUNK_SIZE)
                .fold(0.0f32, |max, &s| max.max(s.abs()));

            max_buffer_amplitude = max_buffer_amplitude.max(max_chunk_amplitude);

            if max_chunk_amplitude < MINIMAL_BUFFER_AMPLITUDE
                || max_chunk_amplitude > MAXIMUL_BUFFER_AMPLITUDE
                || buffer.len() >= max_buffer
            {
                break;
            }
        }

        dbg!(max_buffer_amplitude);
        if max_buffer_amplitude > MINIMAL_BUFFER_AMPLITUDE {
            Some(buffer)
        } else {
            None
        }
    }

    pub fn play(&self) -> anyhow::Result<()> {
        Ok(self.stream.play()?)
    }
}

#[derive(Default)]
pub struct AudioInputStreamBuilder {}

impl AudioInputStreamBuilder {
    pub fn build(self) -> anyhow::Result<AudioInputStream> {
        let (sender, receiver) = mpsc::channel::<Vec<f32>>();

        let host = cpal::default_host();
        let input_device = host
            .default_input_device()
            .context("No input device available")?;
        let input_config = cpal::StreamConfig {
            channels: 1,
            sample_rate: AUDIO_RATE,
            buffer_size: cpal::BufferSize::Default,
        };
        let input_stream = input_device.build_input_stream(
            input_config,
            move |data: &[f32], _| {
                let _ = sender.send(data.to_vec());
            },
            |err| eprintln!("Audio input error: {err}"),
            None,
        )?;

        Ok(AudioInputStream {
            receiver,
            stream: input_stream,
        })
    }
}
