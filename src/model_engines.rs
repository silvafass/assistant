pub mod livekit_wakeword;

use std::collections::VecDeque;

use crate::audio_input::AUDIO_RATE;

const AUDIO_WINDOW_SIZE: usize = (AUDIO_RATE * 2) as usize;
const AUDIO_STRIDE_SIZE: usize = 1280;
const MINIMAL_WINDOW_AMPLITUDE: f32 = 0.09;
const MAXIMUL_WINDOW_AMPLITUDE: f32 = 0.90;

pub trait WakeWordModel {
    fn verify(&mut self, chunk: &[f32]) -> anyhow::Result<bool>;
}

pub trait WithRingBuffer {
    fn into_ring_buffer(self) -> RingBufferWrapper<Self>
    where
        Self: Sized + WakeWordModel,
    {
        RingBufferWrapper::new(self, AUDIO_WINDOW_SIZE)
    }
}

pub struct RingBufferWrapper<T> {
    inner_model: T,
    audio_ring_buffer: VecDeque<f32>,
    accumulated_audio: usize,
}

impl<T: WakeWordModel> RingBufferWrapper<T> {
    pub fn new(model: T, audio_window_size: usize) -> Self {
        Self {
            inner_model: model,
            audio_ring_buffer: VecDeque::from(vec![0f32; audio_window_size]),
            accumulated_audio: 0,
        }
    }

    pub fn verify(&mut self, chunk: &[f32]) -> anyhow::Result<bool> {
        for sample in chunk {
            self.audio_ring_buffer.pop_front();
            self.audio_ring_buffer.push_back(*sample);
            self.accumulated_audio += 1;
        }
        let max_amplitude = self
            .audio_ring_buffer
            .iter()
            .fold(0.0f32, |max, &s| max.max(s.abs()));

        if max_amplitude < MINIMAL_WINDOW_AMPLITUDE || max_amplitude > MAXIMUL_WINDOW_AMPLITUDE {
            self.accumulated_audio = 0;
            return Ok(false);
        }

        let result = if self.accumulated_audio >= AUDIO_STRIDE_SIZE {
            self.accumulated_audio = 0;

            let (front, back) = self.audio_ring_buffer.as_slices();
            self.inner_model.verify(&[front, back].concat())?
        } else {
            false
        };

        if result {
            self.audio_ring_buffer = VecDeque::from(vec![0f32; AUDIO_WINDOW_SIZE]);
        }

        Ok(result)
    }
}
