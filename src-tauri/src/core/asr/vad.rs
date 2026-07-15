use std::path::Path;

pub(crate) const SAMPLE_RATE: i32 = 16_000;
const VAD_WINDOW_SIZE: usize = 512;

#[derive(Clone, Debug)]
pub(crate) struct SpeechSlice {
    pub start_sample: usize,
    pub samples: Vec<f32>,
}

fn push_bounded_segment(
    output: &mut Vec<SpeechSlice>,
    start_sample: usize,
    samples: &[f32],
    max_segment_samples: usize,
) {
    for (index, chunk) in samples.chunks(max_segment_samples).enumerate() {
        output.push(SpeechSlice {
            start_sample: start_sample + index * max_segment_samples,
            samples: chunk.to_vec(),
        });
    }
}

fn drain_vad(
    vad: &sherpa_onnx::VoiceActivityDetector,
    output: &mut Vec<SpeechSlice>,
    max_segment_samples: usize,
) {
    while let Some(segment) = vad.front() {
        let start_sample = segment.start().max(0) as usize;
        let samples = segment.samples().to_vec();
        drop(segment);
        vad.pop();
        push_bounded_segment(output, start_sample, &samples, max_segment_samples);
    }
}

pub(crate) fn detect_speech(
    samples: &[f32],
    vad_model_path: &Path,
    max_segment_seconds: usize,
) -> std::result::Result<Vec<SpeechSlice>, String> {
    if max_segment_seconds == 0 {
        return Err("Silero VAD 最大语音片段时长必须大于 0".into());
    }
    let max_segment_samples = SAMPLE_RATE as usize * max_segment_seconds;
    let config = sherpa_onnx::VadModelConfig {
        silero_vad: sherpa_onnx::SileroVadModelConfig {
            model: Some(vad_model_path.to_string_lossy().to_string()),
            threshold: 0.5,
            min_silence_duration: 0.5,
            min_speech_duration: 0.25,
            window_size: VAD_WINDOW_SIZE as i32,
            max_speech_duration: max_segment_seconds as f32,
        },
        sample_rate: SAMPLE_RATE,
        num_threads: 1,
        provider: Some("cpu".into()),
        debug: false,
        ..Default::default()
    };
    let vad =
        sherpa_onnx::VoiceActivityDetector::create(&config, max_segment_seconds.max(60) as f32)
            .ok_or_else(|| "创建 Silero VAD 失败".to_string())?;
    let mut segments = Vec::new();
    for chunk in samples.chunks(VAD_WINDOW_SIZE) {
        vad.accept_waveform(chunk);
        drain_vad(&vad, &mut segments, max_segment_samples);
    }
    vad.flush();
    drain_vad(&vad, &mut segments, max_segment_samples);
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_split_preserves_offsets_and_maximum_size() {
        let samples = vec![0.25; SAMPLE_RATE as usize * 3 + 7];
        let mut slices = Vec::new();
        push_bounded_segment(&mut slices, 123, &samples, SAMPLE_RATE as usize);
        assert_eq!(slices.len(), 4);
        assert_eq!(slices[0].start_sample, 123);
        assert_eq!(slices[1].start_sample, 123 + SAMPLE_RATE as usize);
        assert_eq!(slices.last().unwrap().samples.len(), 7);
    }
}
