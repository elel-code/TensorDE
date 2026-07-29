//! Retained implementation of Wallpaper Engine's canonical PCM spectrum producer.

use std::f32::consts::PI;

use crate::engine::scene::StereoSpectrum64;

const CANONICAL_BANDS: usize = 64;
const SAMPLE_RATE_REFERENCE: f32 = 44_100.0;
const TRANSFORM_SCALE: f32 = 30.0;
const BIN_SCALE: f32 = 10.0;
const EXPONENT: f32 = 0.25;
const WINDOW_BIAS: f32 = f32::from_bits(0x3f00_4189);
const PCM_COMPLEX_SCALE: f32 = 127.0;
const INPUT_VOLUME_SCALE: f32 = 0.001;
pub(super) const DEFAULT_INPUT_VOLUME: f32 = 1.0;
const GROUP_COUNT: usize = 16;
const GROUP_WIDTH: usize = 8;
const SILENCE_EPSILON: f32 = 0.0001;
const ENVELOPE_FLOOR: f32 = 0.001;

#[derive(Debug, Clone, Copy, Default)]
struct Complex {
    real: f32,
    imaginary: f32,
}

impl Complex {
    const ZERO: Self = Self {
        real: 0.0,
        imaginary: 0.0,
    };

    fn from_angle(angle: f32) -> Self {
        Self {
            real: angle.cos(),
            imaginary: angle.sin(),
        }
    }

    fn multiply(self, right: Self) -> Self {
        Self {
            real: self.real * right.real - self.imaginary * right.imaginary,
            imaginary: self.real * right.imaginary + self.imaginary * right.real,
        }
    }
}

#[derive(Debug)]
struct ArbitraryFftPlan {
    transform_size: usize,
    negative_chirp: Vec<Complex>,
    convolution_kernel_fft: Vec<Complex>,
    scratch: Vec<Complex>,
}

impl ArbitraryFftPlan {
    fn new(transform_size: usize) -> Self {
        let convolution_size = (2 * transform_size - 1).next_power_of_two();
        let negative_chirp = (0..transform_size)
            .map(|index| {
                let phase = -PI * (index as f32 * index as f32) / transform_size as f32;
                Complex::from_angle(phase)
            })
            .collect::<Vec<_>>();
        let mut convolution_kernel_fft = vec![Complex::ZERO; convolution_size];
        convolution_kernel_fft[0] = Complex::from_angle(0.0);
        for index in 1..transform_size {
            let phase = PI * (index as f32 * index as f32) / transform_size as f32;
            let value = Complex::from_angle(phase);
            convolution_kernel_fft[index] = value;
            convolution_kernel_fft[convolution_size - index] = value;
        }
        fft_power_of_two(&mut convolution_kernel_fft, false);
        Self {
            transform_size,
            negative_chirp,
            convolution_kernel_fft,
            scratch: vec![Complex::ZERO; convolution_size],
        }
    }

    fn transform(&mut self, input: &[Complex], output: &mut [Complex]) {
        debug_assert_eq!(input.len(), self.transform_size);
        debug_assert_eq!(output.len(), self.transform_size);
        self.scratch.fill(Complex::ZERO);
        for (destination, (value, chirp)) in self
            .scratch
            .iter_mut()
            .zip(input.iter().zip(&self.negative_chirp))
        {
            *destination = value.multiply(*chirp);
        }
        fft_power_of_two(&mut self.scratch, false);
        for (value, kernel) in self.scratch.iter_mut().zip(&self.convolution_kernel_fft) {
            *value = value.multiply(*kernel);
        }
        fft_power_of_two(&mut self.scratch, true);
        for (index, destination) in output.iter_mut().enumerate() {
            *destination = self.scratch[index].multiply(self.negative_chirp[index]);
        }
    }
}

fn fft_power_of_two(values: &mut [Complex], inverse: bool) {
    debug_assert!(values.len().is_power_of_two());
    let mut reversed = 0usize;
    for index in 1..values.len() {
        let mut bit = values.len() >> 1;
        while reversed & bit != 0 {
            reversed ^= bit;
            bit >>= 1;
        }
        reversed ^= bit;
        if index < reversed {
            values.swap(index, reversed);
        }
    }

    let mut width = 2;
    while width <= values.len() {
        let angle = if inverse {
            2.0 * PI / width as f32
        } else {
            -2.0 * PI / width as f32
        };
        let step = Complex::from_angle(angle);
        for base in (0..values.len()).step_by(width) {
            let mut twiddle = Complex::from_angle(0.0);
            for lane in 0..width / 2 {
                let even = values[base + lane];
                let odd = values[base + lane + width / 2].multiply(twiddle);
                values[base + lane] = Complex {
                    real: even.real + odd.real,
                    imaginary: even.imaginary + odd.imaginary,
                };
                values[base + lane + width / 2] = Complex {
                    real: even.real - odd.real,
                    imaginary: even.imaginary - odd.imaginary,
                };
                twiddle = twiddle.multiply(step);
            }
        }
        width *= 2;
    }
    if inverse {
        let reciprocal = 1.0 / values.len() as f32;
        for value in values {
            value.real *= reciprocal;
            value.imaginary *= reciprocal;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PcmSpectrumError {
    ZeroSampleRate,
    ZeroChannels,
}

#[derive(Debug)]
pub(super) struct PcmSpectrumProducer {
    channels: usize,
    input_volume: f32,
    input_threshold: f32,
    transform_size: usize,
    bin_count: usize,
    capture_frames: usize,
    captured_frames: usize,
    left_input: Vec<Complex>,
    right_input: Vec<Complex>,
    left_fft: Vec<Complex>,
    right_fft: Vec<Complex>,
    plan: ArbitraryFftPlan,
}

#[derive(Debug)]
pub(super) struct SpectrumNormalizer {
    envelopes: [f32; GROUP_COUNT],
    smoothed: [f32; 128],
    output: [f32; 128],
    initialized: bool,
}

impl Default for SpectrumNormalizer {
    fn default() -> Self {
        Self {
            envelopes: [0.0; GROUP_COUNT],
            smoothed: [0.0; 128],
            output: [0.0; 128],
            initialized: false,
        }
    }
}

impl SpectrumNormalizer {
    pub(super) fn normalize(
        &mut self,
        raw: StereoSpectrum64,
        effective_dt: f32,
    ) -> StereoSpectrum64 {
        let mut input = [0.0f32; 128];
        input[..64].copy_from_slice(&raw.left);
        input[64..].copy_from_slice(&raw.right);
        let group_peaks = std::array::from_fn::<_, GROUP_COUNT, _>(|group| {
            input[group * GROUP_WIDTH..(group + 1) * GROUP_WIDTH]
                .iter()
                .copied()
                .fold(0.0f32, f32::max)
        });
        let global_peak = group_peaks.iter().copied().fold(0.0f32, f32::max);
        if global_peak < SILENCE_EPSILON {
            return StereoSpectrum64::ZERO;
        }
        if !self.initialized {
            self.envelopes.fill(1.0);
            self.initialized = true;
        }
        let effective_dt = effective_dt.clamp(0.0001, 0.25);
        for (group, envelope) in self.envelopes.iter_mut().enumerate() {
            let target = group_peaks[group].max(global_peak * 0.333);
            let difference = target - *envelope;
            if difference.abs() <= SILENCE_EPSILON {
                *envelope = target;
            } else if difference > 0.0 {
                *envelope += difference.min(effective_dt);
            } else {
                *envelope += difference.max(-0.5 * effective_dt);
            }
        }
        let smooth_factor = (20.0 * effective_dt).min(1.0);
        let minimum_delta = (-40.0 * effective_dt).max(-1.0);
        let maximum_delta = (40.0 * effective_dt).min(1.0);
        for index in 0..128 {
            let reciprocal =
                approximate_reciprocal(self.envelopes[index / GROUP_WIDTH].max(ENVELOPE_FLOOR));
            let normalized = input[index] * reciprocal;
            self.smoothed[index] += (normalized - self.smoothed[index]) * smooth_factor;
            let delta =
                (self.smoothed[index] - self.output[index]).clamp(minimum_delta, maximum_delta);
            self.output[index] += delta;
        }
        StereoSpectrum64 {
            left: self.output[..64].try_into().expect("64 left spectrum bins"),
            right: self.output[64..]
                .try_into()
                .expect("64 right spectrum bins"),
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn approximate_reciprocal(value: f32) -> f32 {
    use std::arch::x86_64::{_mm_cvtss_f32, _mm_rcp_ss, _mm_set_ss};

    // SAFETY: these baseline SSE instructions are available on every x86_64 target.
    unsafe { _mm_cvtss_f32(_mm_rcp_ss(_mm_set_ss(value))) }
}

#[cfg(not(target_arch = "x86_64"))]
compile_error!("the recovered spectrum normalizer requires x86_64 rcpps semantics");

impl PcmSpectrumProducer {
    pub(super) fn new(
        sample_rate: u32,
        channels: u32,
        input_volume: f32,
        input_threshold: f32,
    ) -> Result<Self, PcmSpectrumError> {
        if sample_rate == 0 {
            return Err(PcmSpectrumError::ZeroSampleRate);
        }
        if channels == 0 {
            return Err(PcmSpectrumError::ZeroChannels);
        }
        let sample_rate_scale = (sample_rate as f32 / SAMPLE_RATE_REFERENCE).max(1.0);
        let transform_size =
            (sample_rate_scale * CANONICAL_BANDS as f32 * TRANSFORM_SCALE) as usize;
        let bin_count = (CANONICAL_BANDS as f32 * BIN_SCALE) as usize;
        let capture_frames = (transform_size as f32
            - (BIN_SCALE / TRANSFORM_SCALE) * transform_size as f32)
            as usize;
        let baseline = pcm_sample_to_complex(0.0);
        Ok(Self {
            channels: channels as usize,
            input_volume,
            input_threshold: input_threshold * INPUT_VOLUME_SCALE,
            transform_size,
            bin_count,
            capture_frames,
            captured_frames: 0,
            left_input: vec![baseline; transform_size],
            right_input: vec![baseline; transform_size],
            left_fft: vec![Complex::ZERO; transform_size],
            right_fft: vec![Complex::ZERO; transform_size],
            plan: ArbitraryFftPlan::new(transform_size),
        })
    }

    pub(super) fn push_interleaved(&mut self, samples: &[f32]) -> Option<StereoSpectrum64> {
        let mut samples = &samples[..samples.len() / self.channels * self.channels];
        let mut latest = None;
        while !samples.is_empty() {
            let available_frames = samples.len() / self.channels;
            let copied_frames = available_frames.min(self.capture_frames - self.captured_frames);
            let copied_samples = copied_frames * self.channels;
            let block = &samples[..copied_samples];
            if !self.block_passes_threshold(block, copied_frames) {
                self.reset_inputs();
                latest = Some(StereoSpectrum64::ZERO);
                samples = &samples[copied_samples..];
                continue;
            }
            for frame in 0..copied_frames {
                let source = frame * self.channels;
                let destination = self.captured_frames + frame;
                self.left_input[destination] = pcm_sample_to_complex(block[source]);
                if self.channels >= 2 {
                    self.right_input[destination] = pcm_sample_to_complex(block[source + 1]);
                }
            }
            self.captured_frames += copied_frames;
            samples = &samples[copied_samples..];
            if self.captured_frames != self.capture_frames {
                continue;
            }

            self.plan.transform(&self.left_input, &mut self.left_fft);
            let left = reduce_fft_channel(
                &self.left_fft,
                self.bin_count,
                self.transform_size,
                self.input_volume,
            );
            let right = if self.channels >= 2 {
                self.plan.transform(&self.right_input, &mut self.right_fft);
                reduce_fft_channel(
                    &self.right_fft,
                    self.bin_count,
                    self.transform_size,
                    self.input_volume,
                )
            } else {
                left
            };
            self.reset_inputs();
            latest = Some(StereoSpectrum64 { left, right });
        }
        latest
    }

    fn block_passes_threshold(&self, samples: &[f32], frames: usize) -> bool {
        if self.input_threshold <= f32::EPSILON {
            return true;
        }
        let positive_peak = (0..frames)
            .map(|frame| samples[frame * self.channels])
            .fold(0.0f32, f32::max);
        self.input_threshold <= positive_peak
    }

    fn reset_inputs(&mut self) {
        let baseline = pcm_sample_to_complex(0.0);
        self.left_input.fill(baseline);
        self.right_input.fill(baseline);
        self.captured_frames = 0;
    }

    #[cfg(test)]
    fn dimensions(&self) -> (usize, usize, usize) {
        (self.transform_size, self.bin_count, self.capture_frames)
    }
}

fn pcm_sample_to_complex(sample: f32) -> Complex {
    let mapped = sample * PCM_COMPLEX_SCALE + PCM_COMPLEX_SCALE;
    Complex {
        real: mapped,
        imaginary: 1.0 / mapped,
    }
}

fn reduce_fft_channel(
    fft: &[Complex],
    bin_count: usize,
    transform_size: usize,
    input_volume: f32,
) -> [f32; 64] {
    let mut output = [0.0f32; 64];
    let inverse_bin_range = 1.0 / (bin_count - 1) as f32;
    let mut previous_band = 0usize;
    for (bin, value) in fft.iter().enumerate().take(bin_count).skip(1) {
        let mut magnitude_squared = value.real * value.real + value.imaginary * value.imaginary;
        if !magnitude_squared.is_finite() {
            magnitude_squared = 0.0;
        }
        let normalized = (bin - 1) as f32 * inverse_bin_range;
        let computed_band = ((64.0 * normalized.powf(EXPONENT)) as i32 & 0x3f) as usize;
        let band = computed_band.min(previous_band + 1);
        previous_band = band;
        let weight = WINDOW_BIAS - (1.0 - WINDOW_BIAS) * (PI * normalized).cos();
        output[band] = output[band].max((magnitude_squared * weight).sqrt());
    }
    let scale =
        input_volume * INPUT_VOLUME_SCALE * bin_count as f32 / (0.5 * transform_size as f32);
    for value in &mut output {
        *value *= scale;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct_dft(input: &[Complex]) -> Vec<Complex> {
        (0..input.len())
            .map(|frequency| {
                input
                    .iter()
                    .enumerate()
                    .fold(Complex::ZERO, |sum, (sample, value)| {
                        let angle =
                            -2.0 * PI * frequency as f32 * sample as f32 / input.len() as f32;
                        let term = value.multiply(Complex::from_angle(angle));
                        Complex {
                            real: sum.real + term.real,
                            imaginary: sum.imaginary + term.imaginary,
                        }
                    })
            })
            .collect()
    }

    #[test]
    fn arbitrary_fft_matches_the_dft_definition() {
        let input = [
            Complex {
                real: 0.25,
                imaginary: -0.5,
            },
            Complex {
                real: 1.0,
                imaginary: 0.125,
            },
            Complex {
                real: -0.75,
                imaginary: 0.25,
            },
            Complex {
                real: 0.5,
                imaginary: 0.0,
            },
            Complex {
                real: 0.125,
                imaginary: 0.75,
            },
        ];
        let expected = direct_dft(&input);
        let mut actual = vec![Complex::ZERO; input.len()];
        ArbitraryFftPlan::new(input.len()).transform(&input, &mut actual);
        for (actual, expected) in actual.iter().zip(expected) {
            assert!((actual.real - expected.real).abs() < 0.00001);
            assert!((actual.imaginary - expected.imaginary).abs() < 0.00001);
        }
    }

    #[test]
    fn recovered_dimensions_follow_float32_truncation() {
        let producer = PcmSpectrumProducer::new(44_100, 2, 50.0, 0.0).unwrap();
        assert_eq!(producer.dimensions(), (1920, 640, 1280));
        let producer = PcmSpectrumProducer::new(48_000, 2, 50.0, 0.0).unwrap();
        assert_eq!(producer.dimensions(), (2089, 640, 1392));
    }

    #[test]
    fn different_stereo_tones_do_not_alias_left_and_right() {
        let mut producer = PcmSpectrumProducer::new(48_000, 2, 50.0, 0.0).unwrap();
        let frames = producer.capture_frames;
        let mut samples = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            let time = frame as f32 / 48_000.0;
            samples.push(0.5 * (2.0 * PI * 440.0 * time).sin());
            samples.push(0.5 * (2.0 * PI * 1_200.0 * time).sin());
        }
        let spectrum = producer.push_interleaved(&samples).expect("one snapshot");
        let left_peak = spectrum
            .left
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .unwrap()
            .0;
        let right_peak = spectrum
            .right
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .unwrap()
            .0;
        assert_ne!(left_peak, right_peak);
        assert!(spectrum.left[left_peak] > spectrum.right[left_peak]);
        assert!(spectrum.right[right_peak] > spectrum.left[right_peak]);
    }

    #[test]
    fn mono_input_is_the_only_producer_path_that_copies_left_to_right() {
        let mut producer = PcmSpectrumProducer::new(44_100, 1, 50.0, 0.0).unwrap();
        let samples = (0..producer.capture_frames)
            .map(|frame| {
                let time = frame as f32 / 44_100.0;
                0.5 * (2.0 * PI * 880.0 * time).sin()
            })
            .collect::<Vec<_>>();
        let spectrum = producer.push_interleaved(&samples).expect("one snapshot");
        assert_eq!(spectrum.left, spectrum.right);
    }
}
