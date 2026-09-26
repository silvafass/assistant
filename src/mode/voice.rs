use std::sync::mpsc;
use std::{collections::VecDeque, time::Instant};

use anyhow::Context;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hf_hub::HFClient;
use livekit_wakeword::WakeWordModel;
use rodio::{ChannelCount, SampleRate};

use crate::providers::Compatibility;

const AUDIO_RATE: u32 = 16_000;
const WEKEWORD_AUDIO_WINDOW_SIZE: usize = 16_000 * 2;
const WEKEWORD_AUDIO_STRIDE_SIZE: usize = 1280;
const MINIMAL_WINDOW_AMPLITUDE: i16 = 700;

pub async fn run(
    _model: &str,
    _input: Option<&str>,
    _compatibility: Compatibility,
    _api_base_url: Option<&str>,
) -> anyhow::Result<()> {
    let client = HFClient::new()?;
    let repo = client.model("silvafass", "wakeword-assistant");
    let wekeword_models_paths = [
        repo.download_file()
            .filename("livekit-wakeword_assistant_pt-br.onnx")
            .send()
            .await?,
        repo.download_file()
            .filename("livekit-wakeword_assistant_en-us.onnx")
            .send()
            .await?,
    ];

    let mut wekeword_model = WakeWordModel::new(wekeword_models_paths.as_slice(), AUDIO_RATE)?;

    let (sender, receiver) = mpsc::channel::<Vec<i16>>();

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
        move |data: &[i16], _| {
            let _ = sender.send(data.to_vec());
        },
        |err| eprintln!("Audio input error: {err}"),
        None,
    )?;
    input_stream.play()?;

    let mut wakeword_audio_ring_buffer: VecDeque<i16> =
        VecDeque::from(vec![0i16; WEKEWORD_AUDIO_WINDOW_SIZE]);
    let mut wakeword_audio_accumulated_audio = 0;

    println!("Listening for wake word...");
    while let Ok(chunk_wakeword) = receiver.recv() {
        for sample in chunk_wakeword {
            wakeword_audio_ring_buffer.pop_front();
            wakeword_audio_ring_buffer.push_back(sample);
            wakeword_audio_accumulated_audio += 1;
        }

        let max_amplitude = wakeword_audio_ring_buffer
            .iter()
            .map(|&s| s.abs())
            .max()
            .unwrap_or(0);

        if max_amplitude < MINIMAL_WINDOW_AMPLITUDE {
            wakeword_audio_accumulated_audio = 0;
            continue;
        }

        let wakeword_predictions = if wakeword_audio_accumulated_audio >= WEKEWORD_AUDIO_STRIDE_SIZE
        {
            wakeword_audio_accumulated_audio = 0;

            let window: Vec<i16> = wakeword_audio_ring_buffer.iter().copied().collect();

            let start = Instant::now();
            let predictions = wekeword_model.predict(&window)?;
            dbg!(start.elapsed());
            predictions
        } else {
            continue;
        };

        if wakeword_predictions
            .iter()
            .find(|(_, score)| **score >= 0.5)
            .is_some()
        {
            for (name, score) in wakeword_predictions {
                if score >= 0.00 {
                    println!("Wake word detected: {name} (Score: {:.2})", score);
                }
            }

            println!("Listening for command...");
            let mut command_audio: Vec<i16> = Vec::new();
            while let Ok(chunk_command) = receiver.recv() {
                command_audio.extend_from_slice(&chunk_command);

                const COMMAND_AUDIO_CHUNK_SIZE: usize = 16_000 * 3;
                let max_amplitude = command_audio
                    .last_chunk::<COMMAND_AUDIO_CHUNK_SIZE>()
                    .unwrap_or(&[MINIMAL_WINDOW_AMPLITUDE; COMMAND_AUDIO_CHUNK_SIZE])
                    .iter()
                    .take(COMMAND_AUDIO_CHUNK_SIZE)
                    .map(|&s| s.abs())
                    .max()
                    .unwrap_or(0);

                if max_amplitude < MINIMAL_WINDOW_AMPLITUDE || command_audio.len() >= (16_000 * 30)
                {
                    break;
                }
            }

            println!("Playing wake word...");
            let mut output_device = rodio::DeviceSinkBuilder::open_default_sink()?;
            let player = rodio::Player::connect_new(output_device.mixer());

            let wakeword_audio: Vec<i16> = wakeword_audio_ring_buffer.iter().copied().collect();
            wakeword_audio_ring_buffer = VecDeque::from(vec![0i16; WEKEWORD_AUDIO_WINDOW_SIZE]);
            let wakeword_audio_f32: Vec<f32> = wakeword_audio
                .iter()
                .map(|&s| s as f32 / i16::MAX as f32)
                .collect();
            let source = rodio::buffer::SamplesBuffer::new(
                ChannelCount::new(1).unwrap(),
                SampleRate::new(AUDIO_RATE).unwrap(),
                wakeword_audio_f32,
            );
            player.append(source);
            player.sleep_until_end();

            println!("Playing command...");
            let command_audio_f32: Vec<f32> = command_audio
                .iter()
                .map(|&s| s as f32 / i16::MAX as f32)
                .collect();
            let source = rodio::buffer::SamplesBuffer::new(
                ChannelCount::new(1).unwrap(),
                SampleRate::new(AUDIO_RATE).unwrap(),
                command_audio_f32,
            );
            player.append(source);
            player.sleep_until_end();

            output_device.log_on_drop(false);

            println!("Listening for wake word again...");
        }
    }

    Ok(())
}
