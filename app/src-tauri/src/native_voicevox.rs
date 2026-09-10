//! VOICEVOX 引擎 sidecar:日文課綱的語音合成。
//! 引擎是官方 HTTP 服務(spawn 子行程於 127.0.0.1 高位 port),
//! start_practice(ja)時啟動、stop_practice 與 app 退出時收掉,避免孤兒行程;
//! 合成走 audio_query → synthesis 兩段 API,輸出 wav 沿用既有 base64 JSON 合約。
//! 聲線授權:VOICEVOX 條款要求標注「VOICEVOX:角色名」(README 技術棧已標)。

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

use crate::{data_dir, log_error_line, log_info_line, podcasts_dir};

/// 避開 VOICEVOX 桌面版預設的 50021,降低相撞機率
const PORT: u16 = 50221;
/// Phase 1 固定聲線(皆ノーマル):女=春日部つむぎ、男=玄野武宏
const SPEAKER_F: i32 = 8;
const SPEAKER_M: i32 = 11;

pub struct JaEngine {
	child: Child,
	client: reqwest::Client,
	/// 講者 → style id;性別歸戶一次後 session 內固定
	voice_map: HashMap<String, i32>,
}

pub struct JaTtsState(pub tokio::sync::Mutex<Option<JaEngine>>);

fn engine_dir() -> std::path::PathBuf {
	data_dir().join("models").join("voicevox_engine")
}

pub fn engine_installed() -> bool {
	engine_dir().join("run").exists()
}

fn base_url() -> String {
	format!("http://127.0.0.1:{}", PORT)
}

/// 啟動引擎並輪詢 /version 至就緒(冷啟約數秒~數十秒)
pub async fn ensure_started(state: &JaTtsState) -> Result<(), String> {
	let mut guard = state.0.lock().await;
	if guard.is_some() {
		return Ok(());
	}
	if !engine_installed() {
		return Err("需要下載日文語音引擎(約 1.8GB)".to_string());
	}
	let start = std::time::Instant::now();
	let mut child = Command::new(engine_dir().join("run"))
		.args(["--host", "127.0.0.1", "--port", &PORT.to_string()])
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.map_err(|e| format!("啟動 VOICEVOX 引擎失敗: {}", e))?;

	let client = reqwest::Client::new();
	loop {
		if let Ok(resp) = client
			.get(format!("{}/version", base_url()))
			.timeout(std::time::Duration::from_secs(2))
			.send()
			.await
		{
			if resp.status().is_success() {
				break;
			}
		}
		if start.elapsed().as_secs() > 90 {
			let _ = child.kill();
			let _ = child.wait();
			log_error_line("practice", "VOICEVOX 引擎啟動逾時");
			return Err("VOICEVOX 引擎啟動逾時,請重試".to_string());
		}
		tokio::time::sleep(std::time::Duration::from_millis(800)).await;
	}
	log_info_line(
		"practice",
		&format!("VOICEVOX 引擎就緒 (耗時 {:.1}s)", start.elapsed().as_secs_f32()),
	);
	*guard = Some(JaEngine { child, client, voice_map: HashMap::new() });
	Ok(())
}

pub async fn shutdown(state: &JaTtsState) {
	if let Some(mut engine) = state.0.lock().await.take() {
		let _ = engine.child.kill();
		let _ = engine.child.wait();
	}
}

/// app 退出路徑(非 async context):盡力收割
pub fn shutdown_blocking(state: &JaTtsState) {
	if let Ok(mut guard) = state.0.try_lock() {
		if let Some(mut engine) = guard.take() {
			let _ = engine.child.kill();
			let _ = engine.child.wait();
		}
	}
}

/// 日文版 play_line:與 kokoro 路徑同一份 JSON 合約
pub async fn play_line_ja(state: &JaTtsState, folder: &str, index: i32) -> Result<serde_json::Value, String> {
	let (speaker, text, total) = crate::native_tts::read_line(folder, index)?;
	let mut guard = state.0.lock().await;
	let engine = guard.as_mut().ok_or("練習模式未啟動")?;

	let sid = resolve_ja_sid(engine, folder, &speaker);
	let query = engine
		.client
		.post(format!("{}/audio_query", base_url()))
		.query(&[("text", text.as_str()), ("speaker", &sid.to_string())])
		.timeout(std::time::Duration::from_secs(30))
		.send()
		.await
		.and_then(|r| r.error_for_status())
		.map_err(|e| format!("audio_query 失敗: {}", e))?
		.text()
		.await
		.map_err(|e| format!("audio_query 失敗: {}", e))?;

	let wav = engine
		.client
		.post(format!("{}/synthesis?speaker={}", base_url(), sid))
		.header("Content-Type", "application/json")
		.body(query)
		.timeout(std::time::Duration::from_secs(120))
		.send()
		.await
		.and_then(|r| r.error_for_status())
		.map_err(|e| {
			log_error_line("practice", &format!("[{}] 第 {} 行 VOICEVOX 合成失敗: {}", folder, index + 1, e));
			"無法產生音訊".to_string()
		})?
		.bytes()
		.await
		.map_err(|e| format!("合成失敗: {}", e))?
		.to_vec();

	let wav = pad_wav_lead_in(wav, 0.4);
	Ok(serde_json::json!({
		"speaker": speaker,
		"text": text,
		// 詞級振り仮名:[[詞面, 讀音|null], ...],前端渲染 <ruby>
		"ruby": crate::furigana::annotate(&text),
		"audio": BASE64.encode(&wav),
		"index": index,
		"total": total,
	}))
}

/// 性別對映:correction.json 的 f/m → 固定聲線;未知則首見輪流(F 先)
fn resolve_ja_sid(engine: &mut JaEngine, folder: &str, speaker: &str) -> i32 {
	if let Some(sid) = engine.voice_map.get(speaker) {
		return *sid;
	}
	let gender = crate::native_tts::speaker_gender(&podcasts_dir().join(folder), speaker);
	let sid = match gender {
		Some('f') => SPEAKER_F,
		Some('m') => SPEAKER_M,
		_ => {
			let f_count = engine.voice_map.values().filter(|v| **v == SPEAKER_F).count();
			let m_count = engine.voice_map.values().filter(|v| **v == SPEAKER_M).count();
			if f_count <= m_count { SPEAKER_F } else { SPEAKER_M }
		}
	};
	engine.voice_map.insert(speaker.to_string(), sid);
	sid
}

/// wav 開頭墊靜音(與 kokoro 路徑的 0.4s 同款,理由見 native_tts::wav_bytes):
/// 掃描 data chunk 插入零樣本並修正 RIFF/data 長度;非預期格式時原樣返回
fn pad_wav_lead_in(mut wav: Vec<u8>, secs: f32) -> Vec<u8> {
	if wav.len() < 44 {
		return wav;
	}
	let Some(p) = wav.windows(4).position(|w| w == b"data") else {
		return wav;
	};
	if p + 8 > wav.len() {
		return wav;
	}
	let channels = u16::from_le_bytes([wav[22], wav[23]]) as u32;
	let rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
	let bits = u16::from_le_bytes([wav[34], wav[35]]) as u32;
	if channels == 0 || rate == 0 || bits % 8 != 0 {
		return wav;
	}
	let lead = ((rate as f32 * secs) as u32 * channels * (bits / 8)) as usize;
	let data_size = u32::from_le_bytes([wav[p + 4], wav[p + 5], wav[p + 6], wav[p + 7]]);
	wav.splice(p + 8..p + 8, std::iter::repeat(0u8).take(lead));
	wav[p + 4..p + 8].copy_from_slice(&(data_size + lead as u32).to_le_bytes());
	let riff = u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]);
	wav[4..8].copy_from_slice(&(riff + lead as u32).to_le_bytes());
	wav
}
