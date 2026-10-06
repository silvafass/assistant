pub struct VoiceAgent {}

impl VoiceAgent {
    pub fn is_anwsering(&self) -> bool {
        false
    }

    pub fn pause(&self) {}

    pub fn next_answer(self) -> Option<()> {
        Some(())
    }

    pub fn chat(&self, audio_samples: &[f32]) {
        dbg!(audio_samples.len());
    }
}
