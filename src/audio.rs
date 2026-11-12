use cpal::{SampleFormat, SupportedStreamConfig, SupportedStreamConfigRange};
use songbird::{Call, Config, CoreEvent, driver::DecodeMode, input::Input};

use crate::{Error, args::AudioMode, receiver::Receiver, sender::Sender};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
pub const MAX_BUFFER_SAMPLES: usize = SAMPLE_RATE as usize * CHANNELS / 5;

pub struct Session {
    microphone: Option<(Input, cpal::Stream)>,
    playback: Option<(Receiver, cpal::Stream)>,
}

// CPAL streams stop when dropped, so keep these alive until the session ends.
pub struct Streams {
    _microphone: Option<cpal::Stream>,
    _playback: Option<cpal::Stream>,
}

impl Session {
    pub fn open(mode: AudioMode, self_mute: bool, self_deafen: bool) -> Result<Self, Error> {
        // The virtual microphone is silent and the virtual output discards audio.
        // No audio track or device is needed to keep a voice connection alive.
        let microphone = if mode == AudioMode::Device && !self_mute {
            Some(Sender::capture()?)
        } else {
            None
        };
        let playback = if mode == AudioMode::Device && !self_deafen {
            Some(Receiver::playback()?)
        } else {
            None
        };
        Ok(Self {
            microphone,
            playback,
        })
    }

    pub fn attach(self, call: &mut Call) -> Streams {
        let microphone = self.microphone.map(|(input, stream)| {
            call.play_only_input(input);
            stream
        });
        let playback = self.playback.map(|(receiver, stream)| {
            call.add_global_event(CoreEvent::VoiceTick.into(), receiver);
            stream
        });
        Streams {
            _microphone: microphone,
            _playback: playback,
        }
    }
}

pub fn voice_config(mode: AudioMode, self_deafen: bool) -> Config {
    let decode_mode = if mode == AudioMode::Device && !self_deafen {
        DecodeMode::Decode(Default::default())
    } else {
        // Virtual/deafened sessions do not need PCM decoding or a playback queue.
        DecodeMode::Pass
    };
    Config::default().decode_mode(decode_mode)
}

pub fn select_config(
    configs: impl Iterator<Item = SupportedStreamConfigRange>,
) -> Result<SupportedStreamConfig, Error> {
    configs
        .filter(|config| matches!(config.channels(), 1 | 2))
        .filter(|config| {
            matches!(
                config.sample_format(),
                SampleFormat::F32
                    | SampleFormat::I16
                    | SampleFormat::U16
                    | SampleFormat::I32
                    | SampleFormat::F64
            )
        })
        .filter_map(|config| config.try_with_sample_rate(SAMPLE_RATE))
        .min_by_key(|config| {
            (
                config.channels() != 2,
                config.sample_format() != SampleFormat::F32,
            )
        })
        .ok_or_else(|| "The default audio device must support mono or stereo PCM at 48 kHz".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::SupportedBufferSize;

    #[test]
    fn virtual_audio_never_opens_devices_even_when_unmuted_and_undeafened() {
        for self_mute in [false, true] {
            for self_deafen in [false, true] {
                let session = Session::open(AudioMode::Virtual, self_mute, self_deafen).unwrap();
                assert!(session.microphone.is_none());
                assert!(session.playback.is_none());
                assert_eq!(
                    voice_config(AudioMode::Virtual, self_deafen).decode_mode,
                    DecodeMode::Pass
                );
            }
        }
    }

    #[test]
    fn muted_and_deafened_device_mode_needs_no_devices() {
        let session = Session::open(AudioMode::Device, true, true).unwrap();
        assert!(session.microphone.is_none());
        assert!(session.playback.is_none());
        assert_eq!(
            voice_config(AudioMode::Device, true).decode_mode,
            DecodeMode::Pass
        );
        assert!(
            voice_config(AudioMode::Device, false)
                .decode_mode
                .should_decode()
        );
    }

    fn config(channels: u16, rate: u32, format: SampleFormat) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(channels, rate, rate, SupportedBufferSize::Unknown, format)
    }

    #[test]
    fn prefers_stereo_float_when_available() {
        let configs = [
            config(1, SAMPLE_RATE, SampleFormat::F32),
            config(2, SAMPLE_RATE, SampleFormat::I16),
            config(2, SAMPLE_RATE, SampleFormat::F32),
        ];
        let selected = select_config(configs.into_iter()).unwrap();
        assert_eq!(selected.channels(), 2);
        assert_eq!(selected.sample_format(), SampleFormat::F32);
    }

    #[test]
    fn accepts_mono_microphones() {
        let selected =
            select_config([config(1, SAMPLE_RATE, SampleFormat::I16)].into_iter()).unwrap();
        assert_eq!(selected.channels(), 1);
    }

    #[test]
    fn rejects_incompatible_rates_and_formats() {
        for unsupported in [
            config(2, 44_100, SampleFormat::F32),
            config(6, SAMPLE_RATE, SampleFormat::F32),
            config(2, SAMPLE_RATE, SampleFormat::DsdU8),
        ] {
            assert!(select_config([unsupported].into_iter()).is_err());
        }
    }
}
