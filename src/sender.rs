use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use songbird::input::{Input, RawAdapter, core::io::MediaSource};

use crate::{Error, audio};

pub struct Sender {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    sample: [u8; 4],
    offset: usize,
}

impl Sender {
    pub fn capture() -> Result<(Input, Stream), Error> {
        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let device = cpal::default_host()
            .default_input_device()
            .ok_or("Failed to get default input device")?;
        let supported = audio::select_config(device.supported_input_configs()?)?;
        let config = supported.config();
        let channels = u32::from(config.channels);
        let stream = match supported.sample_format() {
            SampleFormat::F32 => input_stream::<f32>(&device, config, Arc::clone(&buffer)),
            SampleFormat::I16 => input_stream::<i16>(&device, config, Arc::clone(&buffer)),
            SampleFormat::U16 => input_stream::<u16>(&device, config, Arc::clone(&buffer)),
            SampleFormat::I32 => input_stream::<i32>(&device, config, Arc::clone(&buffer)),
            SampleFormat::F64 => input_stream::<f64>(&device, config, Arc::clone(&buffer)),
            format => return Err(format!("Unsupported input sample format: {format}").into()),
        }?;
        stream.play()?;
        let source = Self {
            buffer,
            sample: [0; 4],
            offset: 4,
        };
        Ok((
            RawAdapter::new(source, audio::SAMPLE_RATE, channels).into(),
            stream,
        ))
    }
}

fn input_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    buffer: Arc<Mutex<VecDeque<f32>>>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels);
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            let mut buffer = buffer.lock().expect("Microphone buffer poisoned");
            buffer.extend(data.iter().map(|sample| sample.to_sample::<f32>()));
            // Drop complete old frames if the consumer falls behind.
            let excess =
                buffer.len().saturating_sub(audio::MAX_BUFFER_SAMPLES) / channels * channels;
            buffer.drain(..excess);
        },
        |error| eprintln!("Microphone stream error: {error}"),
        None,
    )
}

impl Read for Sender {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let mut buffer = self
            .buffer
            .lock()
            .map_err(|_| io::Error::other("Microphone buffer poisoned"))?;
        for byte in output.iter_mut() {
            if self.offset == self.sample.len() {
                self.sample = buffer.pop_front().unwrap_or(0.0).to_le_bytes();
                self.offset = 0;
            }
            *byte = self.sample[self.offset];
            self.offset += 1;
        }
        // A live microphone supplies silence on underrun, never end-of-stream.
        Ok(output.len())
    }
}

impl Seek for Sender {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

impl MediaSource for Sender {
    fn is_seekable(&self) -> bool {
        false
    }
    fn byte_len(&self) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn songbird_decodes_live_mono_and_stereo_pcm() {
        use songbird::input::{
            LiveInput,
            codecs::{get_codec_registry, get_probe},
        };
        use symphonia::core::audio::{AudioBufferRef, Signal};

        for channels in [1, 2] {
            let sender = Sender {
                buffer: Arc::new(Mutex::new(VecDeque::from(vec![0.25; 960 * channels]))),
                sample: [0; 4],
                offset: 4,
            };
            let input: Input = RawAdapter::new(sender, audio::SAMPLE_RATE, channels as u32).into();
            let input = input
                .make_playable_async(get_codec_registry(), get_probe())
                .await
                .unwrap();
            let Input::Live(LiveInput::Parsed(mut parsed), _) = input else {
                panic!("Live microphone input was not parsed");
            };
            let packet = parsed.format.next_packet().unwrap();
            let decoded = parsed.decoder.decode(&packet).unwrap();
            assert_eq!(decoded.spec().rate, audio::SAMPLE_RATE);
            assert_eq!(decoded.spec().channels.count(), channels);
            assert_eq!(decoded.frames(), 960);
            let AudioBufferRef::F32(samples) = decoded else {
                panic!("Microphone samples were not decoded as float PCM");
            };
            assert_eq!(samples.chan(0)[0], 0.25);
        }
    }

    #[test]
    fn preserves_samples_across_partial_reads() {
        let mut sender = Sender {
            buffer: Arc::new(Mutex::new(VecDeque::from([0.25, -0.5]))),
            sample: [0; 4],
            offset: 4,
        };
        let mut bytes = [0; 12];
        sender.read_exact(&mut bytes[..3]).unwrap();
        sender.read_exact(&mut bytes[3..5]).unwrap();
        sender.read_exact(&mut bytes[5..]).unwrap();
        assert_eq!(&bytes[..4], &0.25_f32.to_le_bytes());
        assert_eq!(&bytes[4..8], &(-0.5_f32).to_le_bytes());
        assert_eq!(&bytes[8..], &[0; 4]);
    }

    #[test]
    fn microphone_resumes_after_an_underrun() {
        let mut sender = Sender {
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            sample: [0; 4],
            offset: 4,
        };
        let mut silence = [1; 3];
        sender.read_exact(&mut silence).unwrap();
        assert_eq!(silence, [0; 3]);
        sender.buffer.lock().unwrap().push_back(0.5);
        let mut bytes = [0; 5];
        sender.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes[0], 0);
        assert_eq!(&bytes[1..], &0.5_f32.to_le_bytes());
    }
}
