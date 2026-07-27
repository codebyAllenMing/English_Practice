//! 原生 TTS(sherpa-onnx kokoro):取代 scripts/practice.py。
//! 合約與 Python 版一致:play_line 回 {speaker, text, audio(base64 WAV 24kHz), index, total};
//! 聲音依講者「出場順序」從 VOICE_POOL 輪流分配,start_practice 時重置(同一場練習內固定)。

use std::collections::HashMap;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use sherpa_onnx::{GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig};
use tauri::Manager;
use tokio::sync::Mutex;

use crate::{log_error_line, log_info_line, data_dir};

const MODEL_DIR: &str = "kokoro-multi-lang-v1_0";

/// 可用聲音(名稱, sid, 性別);sid 對照 sherpa-onnx kokoro v1_0 speaker id 表
const VOICES: [(&str, i32, char); 12] = [
	("af_heart", 3, 'f'),
	("af_bella", 2, 'f'),
	("af_nova", 7, 'f'),
	("af_sky", 10, 'f'),
	("af_nicole", 6, 'f'),
	("af_aoede", 1, 'f'),
	("am_adam", 11, 'm'),
	("am_michael", 16, 'm'),
	("am_echo", 12, 'm'),
	("am_liam", 15, 'm'),
	("am_eric", 13, 'm'),
	("am_fenrir", 14, 'm'),
];

/// 舊制 fallback:與 practice.py 相同的八聲出場順序輪替(未校正的集數用)
const ROTATION: [i32; 8] = [3, 11, 2, 16, 10, 12, 7, 15];

pub struct TtsEngine {
	tts: OfflineTts,
	voice_map: HashMap<String, i32>, // speaker → sid(session 內固定)
	f_used: usize,
	m_used: usize,
	rot_used: usize,
}

impl TtsEngine {
	/// 測試/驗證用:指定 sid 直接合成,回傳 (樣本, 取樣率)
	pub fn synth_with_sid(&self, text: &str, sid: i32) -> Option<(Vec<f32>, i32)> {
		self.tts
			.generate_with_config(
				text,
				&GenerationConfig { sid, speed: 1.0, ..Default::default() },
				None::<fn(&[f32], f32) -> bool>,
			)
			.map(|a| (a.samples().to_vec(), a.sample_rate()))
	}
}

pub struct TtsState(pub Mutex<Option<TtsEngine>>);

#[tauri::command]
pub async fn start_practice(app: tauri::AppHandle) -> Result<(), String> {
	let state = app.state::<TtsState>();
	let mut guard = state.0.lock().await;
	if guard.is_some() {
		return Ok(());
	}

	let engine = tokio::task::spawn_blocking(load_engine)
		.await
		.map_err(|e| format!("TTS 載入執行緒失敗: {}", e))??;
	*guard = Some(engine);
	Ok(())
}

/// 載入 kokoro 引擎(blocking,約數秒);test bin 也直接呼叫
pub fn load_engine() -> Result<TtsEngine, String> {
	let model_dir = data_dir().join("models").join(MODEL_DIR);
	if !model_dir.join("model.onnx").exists() {
		return Err(format!("缺少 TTS 模型 models/{},請先下載模型", MODEL_DIR));
	}
	let p = |name: &str| Some(model_dir.join(name).to_string_lossy().into_owned());

	let start = std::time::Instant::now();
	let config = OfflineTtsConfig {
		model: sherpa_onnx::OfflineTtsModelConfig {
			kokoro: OfflineTtsKokoroModelConfig {
				model: p("model.onnx"),
				voices: p("voices.bin"),
				tokens: p("tokens.txt"),
				data_dir: p("espeak-ng-data"),
				dict_dir: p("dict"),
				lexicon: p("lexicon-us-en.txt"),
				..Default::default()
			},
			num_threads: 2,
			..Default::default()
		},
		..Default::default()
	};
	let tts = OfflineTts::create(&config).ok_or("載入 kokoro TTS 模型失敗(模型檔可能損壞)")?;
	log_info_line(
		"practice",
		&format!("kokoro TTS 載入完成 (耗時 {:.1}s)", start.elapsed().as_secs_f32()),
	);
	Ok(TtsEngine { tts, voice_map: HashMap::new(), f_used: 0, m_used: 0, rot_used: 0 })
}

#[tauri::command]
pub async fn stop_practice(app: tauri::AppHandle) -> Result<(), String> {
	let state = app.state::<TtsState>();
	// 丟掉引擎釋放模型記憶體;下次 start_practice 重新載入並重置 voice 分配
	*state.0.lock().await = None;
	Ok(())
}

#[tauri::command]
pub async fn play_line(
	app: tauri::AppHandle,
	folder: String,
	index: i32,
) -> Result<serde_json::Value, String> {
	crate::validate_folder(&folder)?;
	let state = app.state::<TtsState>();
	let mut guard = state.0.lock().await;
	let engine = guard.as_mut().ok_or("練習模式未啟動")?;

	// 合成為 CPU-bound(短句約 0.5~2s),block_in_place 避免佔住 async worker
	tokio::task::block_in_place(|| synth_line(engine, &folder, index))
}

/// 讀 word.txt 第 index 行(0-based)→ 分配聲音 → 合成 → 回 practice.py 同款 JSON
pub fn synth_line(engine: &mut TtsEngine, folder: &str, index: i32) -> Result<serde_json::Value, String> {
	let word_path = data_dir().join("podcasts").join(folder).join("word.txt");
	let content = std::fs::read_to_string(&word_path).map_err(|e| format!("讀取 word.txt 失敗: {}", e))?;
	let lines: Vec<&str> = content.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();

	if index < 0 || index as usize >= lines.len() {
		return Err("無效的行數".to_string());
	}
	let (speaker, text) = parse_line(lines[index as usize]);

	if text.is_empty() {
		return Err("無法產生音訊".to_string());
	}

	let sid = resolve_sid(engine, folder, &speaker);

	let audio = engine
		.tts
		.generate_with_config(
			&text,
			&GenerationConfig { sid, speed: 1.0, ..Default::default() },
			None::<fn(&[f32], f32) -> bool>,
		)
		.ok_or_else(|| {
			log_error_line("practice", &format!("[{}] 第 {} 行合成失敗", folder, index + 1));
			"無法產生音訊".to_string()
		})?;

	let wav = wav_bytes(audio.samples(), audio.sample_rate() as u32);
	Ok(serde_json::json!({
		"speaker": speaker,
		"text": text,
		"audio": BASE64.encode(&wav),
		"index": index,
		"total": lines.len(),
	}))
}

/// 選聲優先序:voices.json 手動指定 > correction.json 的 AI 性別(男/女聲池輪流)> 出場順序輪替。
/// 一位講者在同一場練習 session 內固定同一個聲音。
fn resolve_sid(engine: &mut TtsEngine, folder: &str, speaker: &str) -> i32 {
	if let Some(sid) = engine.voice_map.get(speaker) {
		return *sid;
	}
	let dir = data_dir().join("podcasts").join(folder);

	let sid = manual_sid(&dir, speaker)
		.or_else(|| gender_sid(engine, &dir, speaker))
		.unwrap_or_else(|| {
			let sid = ROTATION[engine.rot_used % ROTATION.len()];
			engine.rot_used += 1;
			sid
		});
	engine.voice_map.insert(speaker.to_string(), sid);
	sid
}

/// voices.json:{"講者名": "af_bella", ...}
fn manual_sid(dir: &std::path::Path, speaker: &str) -> Option<i32> {
	let voices = read_json(&dir.join("voices.json"))?;
	let name = voices[speaker].as_str()?;
	VOICES.iter().find(|(n, _, _)| *n == name).map(|(_, sid, _)| *sid)
}

/// correction.json 的 result.speakers[]:{to: 講者名, gender: "m"/"f"} → 對應性別池輪流
fn gender_sid(engine: &mut TtsEngine, dir: &std::path::Path, speaker: &str) -> Option<i32> {
	let correction = read_json(&dir.join("correction.json"))?;
	let speakers = correction["result"]["speakers"].as_array()?;
	let gender = speakers
		.iter()
		.find(|s| s["to"] == speaker)
		.and_then(|s| s["gender"].as_str())?;
	let (pool, used): (Vec<i32>, &mut usize) = match gender {
		"f" => (VOICES.iter().filter(|v| v.2 == 'f').map(|v| v.1).collect(), &mut engine.f_used),
		"m" => (VOICES.iter().filter(|v| v.2 == 'm').map(|v| v.1).collect(), &mut engine.m_used),
		_ => return None,
	};
	let sid = pool[*used % pool.len()];
	*used += 1;
	Some(sid)
}

fn read_json(path: &std::path::Path) -> Option<serde_json::Value> {
	serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// 每集的手動聲音指定(voices.json);不存在時回空物件
#[tauri::command]
pub fn get_voices(folder: String) -> Result<serde_json::Value, String> {
	crate::validate_folder(&folder)?;
	let path = data_dir().join("podcasts").join(&folder).join("voices.json");
	Ok(read_json(&path).unwrap_or_else(|| serde_json::json!({})))
}

#[tauri::command]
pub fn save_voices(folder: String, voices: serde_json::Value) -> Result<(), String> {
	crate::validate_folder(&folder)?;
	let path = data_dir().join("podcasts").join(&folder).join("voices.json");
	std::fs::write(&path, serde_json::to_string_pretty(&voices).map_err(|e| e.to_string())?)
		.map_err(|e| format!("寫入 voices.json 失敗: {}", e))
}

/// 解析 "[Speaker Name]: text";講者名可含空白,無前綴時視為 UNKNOWN
fn parse_line(line: &str) -> (String, String) {
	if let Some(rest) = line.strip_prefix('[') {
		if let Some(pos) = rest.find("]:") {
			let speaker = rest[..pos].to_string();
			let text = rest[pos + 2..].trim().to_string();
			return (speaker, text);
		}
	}
	("UNKNOWN".to_string(), line.to_string())
}

/// f32 [-1,1] 樣本 → 16-bit PCM mono WAV 位元組
fn wav_bytes(samples: &[f32], sample_rate: u32) -> Vec<u8> {
	let data_len = (samples.len() * 2) as u32;
	let mut out = Vec::with_capacity(44 + data_len as usize);
	out.extend_from_slice(b"RIFF");
	out.extend_from_slice(&(36 + data_len).to_le_bytes());
	out.extend_from_slice(b"WAVEfmt ");
	out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
	out.extend_from_slice(&1u16.to_le_bytes()); // PCM
	out.extend_from_slice(&1u16.to_le_bytes()); // mono
	out.extend_from_slice(&sample_rate.to_le_bytes());
	out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
	out.extend_from_slice(&2u16.to_le_bytes()); // block align
	out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
	out.extend_from_slice(b"data");
	out.extend_from_slice(&data_len.to_le_bytes());
	for s in samples {
		out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
	}
	out
}
