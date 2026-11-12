use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use serenity::async_trait;
use songbird::{Event, EventContext, EventHandler};

use crate::{Error, audio};

pub struct Receiver(Arc<Mutex<VecDeque<i16>>>);

impl Receiver {
    pub fn playback() -> Result<(Self, Stream), Error> {
        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("Failed to get default output device")?;
        let supported = audio::select_config(device.supported_output_configs()?)?;
        let config = supported.config();
        let stream = match supported.sample_format() {
            SampleFormat::F32 => output_stream::<f32>(&device, config, Arc::clone(&buffer)),
            SampleFormat::I16 => output_stream::<i16>(&device, config, Arc::clone(&buffer)),
            SampleFormat::U16 => output_stream::<u16>(&device, config, Arc::clone(&buffer)),
            SampleFormat::I32 => output_stream::<i32>(&device, config, Arc::clone(&buffer)),
            SampleFormat::F64 => output_stream::<f64>(&device, config, Arc::clone(&buffer)),
            format => return Err(format!("Unsupported output sample format: {format}").into()),
        }?;
        stream.play()?;
        Ok((Self(buffer), stream))
    }
}

fn output_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    buffer: Arc<Mutex<VecDeque<i16>>>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample + FromSample<i16>,
{
    let channels = usize::from(config.channels);
    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            let mut buffer = buffer.lock().expect("Playback buffer poisoned");
            fill_output(data, channels, &mut buffer);
        },
        |error| eprintln!("Playback stream error: {error}"),
        None,
    )
}

fn fill_output<T: SizedSample + FromSample<i16>>(
    output: &mut [T],
    channels: usize,
    buffer: &mut VecDeque<i16>,
) {
    for frame in output.chunks_mut(channels) {
        let left = buffer.pop_front().unwrap_or(0);
        let right = buffer.pop_front().unwrap_or(0);
        if channels == 1 {
            frame[0] = T::from_sample(((i32::from(left) + i32::from(right)) / 2) as i16);
        } else {
            frame[0] = T::from_sample(left);
            frame[1] = T::from_sample(right);
        }
    }
}

fn mix_audio<'a>(voices: impl Iterator<Item = &'a [i16]>) -> Vec<i16> {
    let mut mixed = Vec::<i32>::new();
    for voice in voices {
        mixed.resize(mixed.len().max(voice.len()), 0);
        for (output, sample) in mixed.iter_mut().zip(voice) {
            *output += i32::from(*sample);
        }
    }
    mixed
        .into_iter()
        .map(|sample| sample.clamp(i16::MIN.into(), i16::MAX.into()) as i16)
        .collect()
}

#[async_trait]
impl EventHandler for Receiver {
    async fn act(&self, context: &EventContext<'_>) -> Option<Event> {
        if let EventContext::VoiceTick(tick) = context {
            let mixed = mix_audio(
                tick.speaking
                    .values()
                    .filter_map(|voice| voice.decoded_voice.as_deref()),
            );
            let mut buffer = self.0.lock().expect("Playback buffer poisoned");
            buffer.extend(mixed);
            let excess = buffer.len().saturating_sub(audio::MAX_BUFFER_SAMPLES) / audio::CHANNELS
                * audio::CHANNELS;
            buffer.drain(..excess);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_simultaneous_speakers_and_clips_peaks() {
        let voices: [&[i16]; 2] = [&[20_000, -20_000, 100], &[20_000, -20_000]];
        assert_eq!(mix_audio(voices.into_iter()), [i16::MAX, i16::MIN, 100]);
    }

    #[test]
    fn converts_stereo_to_mono_and_supplies_silence() {
        let mut buffer = VecDeque::from([1_000, 3_000, -1_000, -3_000]);
        let mut output = [0_i16; 3];
        fill_output(&mut output, 1, &mut buffer);
        assert_eq!(output, [2_000, -2_000, 0]);
    }

    #[test]
    fn unsigned_output_uses_the_correct_silence_level() {
        let mut output = [0_u16; 4];
        fill_output(&mut output, 2, &mut VecDeque::new());
        assert_eq!(output, [32_768; 4]);
    }
}
