//! 原生轉譯管線:whisper.cpp(Metal)ASR + sherpa-onnx speaker diarization。
//! 取代 scripts/transcribe.py(whisperx):同一集約 4 分鐘 vs 15 分鐘,且不需 HF token。
//!
//! 流程:mp3 → ffmpeg 轉 16kHz mono wav → whisper 逐詞時間戳 → diarization →
//! 逐詞掛講者 + 平滑 → 按「講者變更/句尾標點」斷行 → word.raw.txt + word.txt。
//! 參數實測依據見 .claude/memory/plan-split-ai-correction.md(階段 3 spike)。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use sherpa_onnx::{
	FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
	OfflineSpeakerDiarizationSegment, OfflineSpeakerSegmentationModelConfig,
	OfflineSpeakerSegmentationPyannoteModelConfig, SpeakerEmbeddingExtractorConfig, Wave,
};
use tauri::Emitter;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::{log_error_line, log_info_line, project_dir};

const WHISPER_MODEL: &str = "ggml-large-v3-turbo-q5_0.bin";
const SEGMENTATION_MODEL: &str = "sherpa-onnx-pyannote-segmentation-3-0/model.onnx";
const EMBEDDING_MODEL: &str = "nemo_en_titanet_small.onnx";

pub type Progress = Arc<dyn Fn(String) + Send + Sync>;

struct Word {
	start: f32,
	end: f32,
	text: String,
}

#[tauri::command]
pub async fn transcribe_audio(app: tauri::AppHandle, folder: String) -> Result<String, String> {
	let progress_app = app.clone();
	tokio::task::spawn_blocking(move || {
		let progress: Progress = Arc::new(move |msg: String| {
			let _ = progress_app.emit("transcribe-progress", msg);
		});
		run_pipeline(&folder, progress)
	})
	.await
	.map_err(|e| format!("轉譯執行緒失敗: {}", e))?
}

fn models_dir() -> PathBuf {
	project_dir().join("models")
}

fn check_models() -> Result<(), String> {
	for m in [WHISPER_MODEL, SEGMENTATION_MODEL, EMBEDDING_MODEL] {
		if !models_dir().join(m).exists() {
			return Err(format!("缺少模型檔 models/{},請先下載模型", m));
		}
	}
	Ok(())
}

/// macOS 從 Finder 啟動時 PATH 不含 /opt/homebrew/bin,直接找絕對路徑
fn ffmpeg_bin() -> &'static str {
	if std::path::Path::new("/opt/homebrew/bin/ffmpeg").exists() {
		"/opt/homebrew/bin/ffmpeg"
	} else {
		"ffmpeg"
	}
}

pub fn run_pipeline(folder: &str, progress: Progress) -> Result<String, String> {
	let total_timer = Instant::now();
	log_info_line("transcribe", &format!("開始轉譯: {}", folder));

	match run_pipeline_inner(folder, &progress, total_timer) {
		Ok(path) => Ok(path),
		Err(e) => {
			log_error_line("transcribe", &format!("[{}] 轉譯失敗: {}", folder, e));
			Err(e)
		}
	}
}

fn run_pipeline_inner(folder: &str, progress: &Progress, total_timer: Instant) -> Result<String, String> {
	let folder_path = project_dir().join("podcasts").join(folder);
	let audio_path = folder_path.join("podcast.mp3");
	if !audio_path.exists() {
		return Err(format!("找不到音檔 {}", audio_path.display()));
	}
	check_models()?;

	// Step 1: mp3 → 16kHz mono wav(whisper 與 diarization 共用)
	progress("準備音檔...".to_string());
	let wav_path = folder_path.join(".audio16k.tmp.wav");
	let output = std::process::Command::new(ffmpeg_bin())
		.args(["-v", "error", "-y", "-i"])
		.arg(&audio_path)
		.args(["-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le"])
		.arg(&wav_path)
		.output()
		.map_err(|e| format!("無法執行 ffmpeg: {}", e))?;
	if !output.status.success() {
		return Err(format!("ffmpeg 轉檔失敗: {}", String::from_utf8_lossy(&output.stderr)));
	}

	let result = (|| {
		let wave = Wave::read(&wav_path.to_string_lossy())
			.ok_or("讀取 wav 失敗")?;
		let samples = wave.samples().to_vec();
		let audio_secs = samples.len() as f32 / 16000.0;

		// Step 2: whisper ASR(Metal,逐詞時間戳)
		let timer = Instant::now();
		let words = run_whisper(&samples, progress)?;
		log_info_line(
			"transcribe",
			&format!("[{}] whisper 完成 (耗時 {:.1}s, 音檔 {:.1}s, {} 詞)", folder, timer.elapsed().as_secs_f32(), audio_secs, words.len()),
		);

		// Step 3: speaker diarization
		progress("辨識說話者...".to_string());
		let timer = Instant::now();
		let turns = run_diarization(&samples)?;
		log_info_line(
			"transcribe",
			&format!("[{}] diarization 完成 (耗時 {:.1}s, {} 個發言區間)", folder, timer.elapsed().as_secs_f32(), turns.len()),
		);

		// Step 4: 逐詞掛講者 → 平滑 → 斷行
		progress("產生文字檔...".to_string());
		let lines = build_lines(&words, &turns);

		let raw_path = folder_path.join("word.raw.txt");
		let word_path = folder_path.join("word.txt");
		fs::write(&raw_path, lines.join("\n")).map_err(|e| format!("寫入 word.raw.txt 失敗: {}", e))?;
		fs::copy(&raw_path, &word_path).map_err(|e| format!("寫入 word.txt 失敗: {}", e))?;

		log_info_line(
			"transcribe",
			&format!("[{}] word.raw.txt / word.txt 產生完成 ({} 行)", folder, lines.len()),
		);
		log_info_line(
			"transcribe",
			&format!("[{}] 全部完成 (總耗時 {:.1}s)", folder, total_timer.elapsed().as_secs_f32()),
		);
		Ok(format!("podcasts/{}/word.txt", folder))
	})();

	let _ = fs::remove_file(&wav_path);
	result
}

fn run_whisper(samples: &[f32], progress: &Progress) -> Result<Vec<Word>, String> {
	let model_path = models_dir().join(WHISPER_MODEL);
	let ctx = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
		.map_err(|e| format!("載入 whisper 模型失敗: {}", e))?;
	let mut state = ctx.create_state().map_err(|e| format!("建立 whisper state 失敗: {}", e))?;

	let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
	params.set_language(Some("en"));
	params.set_translate(false);
	params.set_print_special(false);
	params.set_print_progress(false);
	params.set_print_realtime(false);
	params.set_print_timestamps(false);
	// 逐詞時間戳:講者分配的精度來源(等同 whisperx 的字級對齊)
	params.set_token_timestamps(true);
	params.set_max_len(1);
	params.set_split_on_word(true);
	let progress_cb = progress.clone();
	params.set_progress_callback_safe(move |pct: i32| {
		progress_cb(format!("轉錄中... {}%", pct));
	});

	state.full(params, samples).map_err(|e| format!("whisper 轉譯失敗: {}", e))?;

	let mut words: Vec<Word> = Vec::new();
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
		words.push(Word {
			start: seg.start_timestamp() as f32 / 100.0,
			end: seg.end_timestamp() as f32 / 100.0,
			text,
		});
	}
	Ok(words)
}

fn run_diarization(samples: &[f32]) -> Result<Vec<OfflineSpeakerDiarizationSegment>, String> {
	// threshold 實測:0.5 會過度切分(32 clusters),1.0 收斂到接近真實講者數;可由 config 覆寫
	let threshold: f32 = fs::read_to_string(project_dir().join("config.json"))
		.ok()
		.and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
		.and_then(|v| v["diarization_threshold"].as_f64())
		.map(|v| v as f32)
		.unwrap_or(1.0);

	let config = OfflineSpeakerDiarizationConfig {
		segmentation: OfflineSpeakerSegmentationModelConfig {
			pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
				model: Some(models_dir().join(SEGMENTATION_MODEL).to_string_lossy().into_owned()),
			},
			num_threads: 4,
			..Default::default()
		},
		embedding: SpeakerEmbeddingExtractorConfig {
			model: Some(models_dir().join(EMBEDDING_MODEL).to_string_lossy().into_owned()),
			num_threads: 4,
			..Default::default()
		},
		clustering: FastClusteringConfig {
			num_clusters: -1,
			threshold,
		},
		..Default::default()
	};
	let sd = OfflineSpeakerDiarization::create(&config).ok_or("初始化 diarization 失敗(模型檔可能損壞)")?;
	let result = sd.process(samples).ok_or("diarization 執行失敗")?;
	Ok(result.sort_by_start_time())
}

/// 逐詞掛講者(重疊最長,無重疊取最近)→ 多數決平滑 → 按「講者變更/句尾標點」斷行
fn build_lines(words: &[Word], turns: &[OfflineSpeakerDiarizationSegment]) -> Vec<String> {
	let mut speakers: Vec<Option<i32>> = words.iter().map(|w| assign_speaker(w, turns)).collect();
	smooth_speakers(&mut speakers);

	let mut first_seen: Vec<i32> = Vec::new();
	let mut lines: Vec<String> = Vec::new();
	let mut cur_speaker: Option<i32> = None;
	let mut cur_words: Vec<&str> = Vec::new();

	for (word, speaker) in words.iter().zip(speakers.iter()) {
		if !cur_words.is_empty() && *speaker != cur_speaker {
			flush_line(&mut cur_words, cur_speaker, &mut first_seen, &mut lines);
		}
		cur_speaker = *speaker;
		cur_words.push(&word.text);
		if word.text.ends_with('.') || word.text.ends_with('?') || word.text.ends_with('!') {
			flush_line(&mut cur_words, cur_speaker, &mut first_seen, &mut lines);
		}
	}
	flush_line(&mut cur_words, cur_speaker, &mut first_seen, &mut lines);
	lines
}

/// 孤立的 1~2 詞講者跳動(token 時間戳抖動)以視窗多數決撫平
fn smooth_speakers(speakers: &mut [Option<i32>]) {
	let n = speakers.len();
	if n < 3 {
		return;
	}
	let orig: Vec<Option<i32>> = speakers.to_vec();
	for i in 0..n {
		let lo = i.saturating_sub(2);
		let hi = (i + 2).min(n - 1);
		let mut counts: HashMap<i32, usize> = HashMap::new();
		for s in orig[lo..=hi].iter().flatten() {
			*counts.entry(*s).or_insert(0) += 1;
		}
		if let Some((&best, &c)) = counts.iter().max_by_key(|(_, c)| **c) {
			// 需嚴格過半才覆寫,平手時保留原判定
			if c * 2 > hi - lo + 1 {
				speakers[i] = Some(best);
			}
		}
	}
}

fn assign_speaker(word: &Word, turns: &[OfflineSpeakerDiarizationSegment]) -> Option<i32> {
	let mut overlap: HashMap<i32, f32> = HashMap::new();
	for t in turns {
		let o = (word.end.min(t.end) - word.start.max(t.start)).max(0.0);
		if o > 0.0 {
			*overlap.entry(t.speaker).or_insert(0.0) += o;
		}
	}
	if let Some((&id, _)) = overlap.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()) {
		return Some(id);
	}

	// 無重疊(詞落在靜音區):取距離最近的發言區間
	let mid = (word.start + word.end) / 2.0;
	turns
		.iter()
		.min_by(|a, b| distance(mid, a).partial_cmp(&distance(mid, b)).unwrap())
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

fn speaker_label(speaker: Option<i32>, first_seen: &mut Vec<i32>) -> String {
	match speaker {
		Some(id) => {
			if !first_seen.contains(&id) {
				first_seen.push(id);
			}
			let idx = first_seen.iter().position(|s| *s == id).unwrap();
			format!("SPEAKER_{:02}", idx)
		}
		None => "UNKNOWN".to_string(),
	}
}

fn flush_line(words: &mut Vec<&str>, speaker: Option<i32>, first_seen: &mut Vec<i32>, lines: &mut Vec<String>) {
	if words.is_empty() {
		return;
	}
	let label = speaker_label(speaker, first_seen);
	lines.push(format!("[{}]: {}", label, words.join(" ")));
	words.clear();
}
