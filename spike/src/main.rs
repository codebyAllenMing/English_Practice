// spike:原生轉譯管線對照實驗
// 流程:16kHz mono WAV → whisper.cpp(Metal)ASR → sherpa-onnx diarization → 講者分配 → word.txt 格式輸出
// 用法:spike-transcribe <audio-16k-mono.wav> <models-dir> <out.txt>

use std::collections::HashMap;
use std::env;
use std::fs;
use std::process::exit;
use std::time::Instant;

use sherpa_onnx::{
	FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
	OfflineSpeakerDiarizationSegment, OfflineSpeakerSegmentationModelConfig,
	OfflineSpeakerSegmentationPyannoteModelConfig, SpeakerEmbeddingExtractorConfig, Wave,
};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

struct AsrSegment {
	start: f32,
	end: f32,
	text: String,
}

fn main() {
	let args: Vec<String> = env::args().collect();
	if args.len() < 4 {
		eprintln!("用法: spike-transcribe <audio-16k-mono.wav> <models-dir> <out.txt> [threshold=0.5] [num_clusters=-1] [threads=4]");
		exit(1);
	}
	let (audio_path, models_dir, out_path) = (&args[1], &args[2], &args[3]);
	let threshold: f32 = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(0.5);
	let num_clusters: i32 = args.get(5).map(|s| s.parse().unwrap()).unwrap_or(-1);
	let threads: i32 = args.get(6).map(|s| s.parse().unwrap()).unwrap_or(4);
	// word 模式:逐詞時間戳 → 按「講者變更 + 句尾標點」斷行(對齊 whisperx 的字級分配)
	let word_mode: bool = args.get(7).map(|s| s == "word").unwrap_or(false);

	let total_timer = Instant::now();

	let wave = Wave::read(audio_path).expect("讀不到 wav(需 16kHz mono PCM)");
	assert_eq!(wave.sample_rate(), 16000, "取樣率必須是 16kHz");
	let samples = wave.samples();
	let audio_secs = samples.len() as f32 / 16000.0;
	println!("音檔長度: {audio_secs:.1}s");

	// ── Whisper ASR(Metal),結果快取於 <audio>.segments.tsv 供調參重跑時跳過 ──
	let cache_path = if word_mode {
		format!("{audio_path}.words.tsv")
	} else {
		format!("{audio_path}.segments.tsv")
	};
	let segments: Vec<AsrSegment> = if let Ok(cached) = fs::read_to_string(&cache_path) {
		let segs: Vec<AsrSegment> = cached
			.lines()
			.filter_map(|l| {
				let mut parts = l.splitn(3, '\t');
				Some(AsrSegment {
					start: parts.next()?.parse().ok()?,
					end: parts.next()?.parse().ok()?,
					text: parts.next()?.to_string(),
				})
			})
			.collect();
		println!("whisper: 使用快取 {cache_path}({} 段)", segs.len());
		segs
	} else {
		let timer = Instant::now();
		let ctx = WhisperContext::new_with_params(
			&format!("{models_dir}/ggml-large-v3-turbo-q5_0.bin"),
			WhisperContextParameters::default(),
		)
		.expect("載入 whisper 模型失敗");
		let mut state = ctx.create_state().expect("建立 whisper state 失敗");
		let load_secs = timer.elapsed().as_secs_f32();

		let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
		params.set_language(Some("en"));
		params.set_translate(false);
		params.set_print_special(false);
		params.set_print_progress(false);
		params.set_print_realtime(false);
		params.set_print_timestamps(false);
		if word_mode {
			params.set_token_timestamps(true);
			params.set_max_len(1);
			params.set_split_on_word(true);
		}

		let timer2 = Instant::now();
		state.full(params, samples).expect("whisper 轉譯失敗");
		let asr_secs = timer2.elapsed().as_secs_f32();

		let mut segs: Vec<AsrSegment> = Vec::new();
		for seg in state.as_iter() {
			let text = seg
				.to_str_lossy()
				.map(|s| s.into_owned())
				.unwrap_or_default()
				.trim()
				.to_string();
			if text.is_empty() {
				continue;
			}
			segs.push(AsrSegment {
				start: seg.start_timestamp() as f32 / 100.0,
				end: seg.end_timestamp() as f32 / 100.0,
				text,
			});
		}
		println!(
			"whisper: 載入 {load_secs:.1}s, 轉譯 {asr_secs:.1}s({:.1}x 即時), {} 段",
			audio_secs / asr_secs,
			segs.len()
		);
		let tsv: Vec<String> = segs
			.iter()
			.map(|s| format!("{}\t{}\t{}", s.start, s.end, s.text))
			.collect();
		let _ = fs::write(&cache_path, tsv.join("\n"));
		segs
	};

	// ── Speaker diarization ──
	let timer = Instant::now();
	let config = OfflineSpeakerDiarizationConfig {
		segmentation: OfflineSpeakerSegmentationModelConfig {
			pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
				model: Some(format!(
					"{models_dir}/sherpa-onnx-pyannote-segmentation-3-0/model.onnx"
				)),
			},
			num_threads: threads,
			..Default::default()
		},
		embedding: SpeakerEmbeddingExtractorConfig {
			model: Some(format!("{models_dir}/nemo_en_titanet_small.onnx")),
			num_threads: threads,
			..Default::default()
		},
		clustering: FastClusteringConfig {
			num_clusters,
			threshold,
		},
		..Default::default()
	};
	println!("diarization 參數: threshold={threshold}, num_clusters={num_clusters}, threads={threads}");
	let sd = OfflineSpeakerDiarization::create(&config).expect("初始化 diarization 失敗");
	let result = sd.process(samples).expect("diarization 失敗");
	let turns = result.sort_by_start_time();
	let diar_secs = timer.elapsed().as_secs_f32();
	println!(
		"diarization: {diar_secs:.1}s, 偵測 {} 位講者, {} 個發言區間",
		result.num_speakers(),
		turns.len()
	);

	// ── 講者分配:重疊時間最長的講者;無重疊時取最近的區間 ──
	let mut first_seen: Vec<i32> = Vec::new();
	let mut lines: Vec<String> = Vec::new();
	if word_mode {
		// 逐詞掛講者,遇「講者變更」或「句尾標點」斷行
		let mut cur_speaker: Option<i32> = None;
		let mut cur_words: Vec<String> = Vec::new();
		for seg in &segments {
			let speaker = assign_speaker(seg, &turns);
			if !cur_words.is_empty() && speaker != cur_speaker {
				flush_line(&mut cur_words, cur_speaker, &mut first_seen, &mut lines);
			}
			cur_speaker = speaker;
			let word = seg.text.trim();
			if word.is_empty() {
				continue;
			}
			cur_words.push(word.to_string());
			if word.ends_with('.') || word.ends_with('?') || word.ends_with('!') {
				flush_line(&mut cur_words, cur_speaker, &mut first_seen, &mut lines);
			}
		}
		flush_line(&mut cur_words, cur_speaker, &mut first_seen, &mut lines);
	} else {
		for seg in &segments {
			let speaker = assign_speaker(seg, &turns);
			let label = speaker_label(speaker, &mut first_seen);
			lines.push(format!("[{label}]: {}", seg.text));
		}
	}

	fs::write(out_path, lines.join("\n")).expect("寫出結果失敗");
	println!(
		"完成: {} 行 → {out_path}(總耗時 {:.1}s)",
		lines.len(),
		total_timer.elapsed().as_secs_f32()
	);
}

fn speaker_label(speaker: Option<i32>, first_seen: &mut Vec<i32>) -> String {
	match speaker {
		Some(id) => {
			if !first_seen.contains(&id) {
				first_seen.push(id);
			}
			let idx = first_seen.iter().position(|s| *s == id).unwrap();
			format!("SPEAKER_{idx:02}")
		}
		None => "UNKNOWN".to_string(),
	}
}

fn flush_line(
	words: &mut Vec<String>,
	speaker: Option<i32>,
	first_seen: &mut Vec<i32>,
	lines: &mut Vec<String>,
) {
	if words.is_empty() {
		return;
	}
	let label = speaker_label(speaker, first_seen);
	lines.push(format!("[{label}]: {}", words.join(" ")));
	words.clear();
}

fn assign_speaker(seg: &AsrSegment, turns: &[OfflineSpeakerDiarizationSegment]) -> Option<i32> {
	let mut overlap: HashMap<i32, f32> = HashMap::new();
	for t in turns {
		let o = (seg.end.min(t.end) - seg.start.max(t.start)).max(0.0);
		if o > 0.0 {
			*overlap.entry(t.speaker).or_insert(0.0) += o;
		}
	}
	if let Some((&id, _)) = overlap
		.iter()
		.max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
	{
		return Some(id);
	}

	// 無重疊(如段落落在靜音區):取距離最近的發言區間
	let mid = (seg.start + seg.end) / 2.0;
	turns
		.iter()
		.min_by(|a, b| {
			let da = distance(mid, a);
			let db = distance(mid, b);
			da.partial_cmp(&db).unwrap()
		})
		.map(|t| t.speaker)
}

fn distance(point: f32, turn: &OfflineSpeakerDiarizationSegment) -> f32 {
	if point < turn.start {
		turn.start - point
	} else if point > turn.end {
		point - turn.end
	} else {
		0.0
	}
}
