use crate::audio_input::AUDIO_RATE;
use rodio::{ChannelCount, SampleRate};

use crate::audio_input::AudioInputStreamBuilder;
use crate::model_engines::WithRingBuffer;
use crate::model_engines::livekit_wakeword::LivekitWakeWordModel;

use crate::providers::Compatibility;
use crate::voice_agent::VoiceAgent;

pub async fn run(
    _model: &str,
    _input: Option<&str>,
    _compatibility: Compatibility,
    _api_base_url: Option<&str>,
) -> anyhow::Result<()> {
    println!("Running in voice mode...");

    let mut wakeword_engine = LivekitWakeWordModel::from_huggingface_paths(&[
        "silvafass/wakeword-assistant/livekit-wakeword_assistant_pt-br.onnx",
        "silvafass/wakeword-assistant/livekit-wakeword_assistant_en-us.onnx",
    ])
    .await?
    .into_ring_buffer();

    let voice_agent = VoiceAgent {};

    let mut listem_for_command = false;

    let mut output_device = rodio::DeviceSinkBuilder::open_default_sink()?;
    let player = rodio::Player::connect_new(output_device.mixer());
    output_device.log_on_drop(false);

    loop {
        let input_stream = AudioInputStreamBuilder::default().build()?;
        input_stream.play()?;

        loop {
            let audio_chunk = match input_stream.next_chunk() {
                Ok(audio_input) => audio_input,
                Err(err) => {
                    println!("Error: {err}");
                    break;
                }
            };

            let wakeword_actived = wakeword_engine.verify(&audio_chunk)?;
            if wakeword_actived {
                dbg!(wakeword_actived);
                if voice_agent.is_anwsering() {
                    voice_agent.pause();
                }
                listem_for_command = true;
            }

            if listem_for_command && !voice_agent.is_anwsering() {
                listem_for_command = false;
                println!("Listening for command...");
                if let Some(voice_command) = input_stream.listen((AUDIO_RATE * 30) as usize) {
                    let source = rodio::buffer::SamplesBuffer::new(
                        ChannelCount::new(1).unwrap(),
                        SampleRate::new(AUDIO_RATE).unwrap(),
                        voice_command.clone(),
                    );
                    player.append(source);
                    player.play();

                    voice_agent.chat(&voice_command[..]);
                    listem_for_command = true;
                }
            }
        }
    }
}
