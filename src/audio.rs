use cpal::{SampleFormat, SupportedStreamConfig, SupportedStreamConfigRange};

use crate::Error;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
pub const MAX_BUFFER_SAMPLES: usize = SAMPLE_RATE as usize * CHANNELS / 5;

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
