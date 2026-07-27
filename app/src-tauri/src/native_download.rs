//! 原生下載:直接 spawn yt-dlp CLI,取代 scripts/download.py——Python 依賴就此歸零。
//! 事件契約沿用:download-title(標題)、download-progress(百分比數字或 "converting")。

use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::time::Instant;

use tauri::Emitter;

use crate::{log_error_line, log_info_line, data_dir};

#[derive(serde::Serialize)]
pub struct TitleInfo {
	pub title: String,
	pub folder: String,
}

fn yt_dlp_bin() -> std::path::PathBuf {
	crate::find_tool("yt-dlp").unwrap_or_else(|| "yt-dlp".into())
}

/// 外部 CLI 檢查:缺件時前端顯示安裝引導(brew install yt-dlp ffmpeg)
#[tauri::command]
pub fn tools_status() -> serde_json::Value {
	serde_json::json!({
		"ytDlp": crate::find_tool("yt-dlp").is_some(),
		"ffmpeg": crate::find_tool("ffmpeg").is_some(),
	})
}

#[tauri::command]
pub async fn fetch_title(url: String) -> Result<TitleInfo, String> {
	tokio::task::spawn_blocking(move || fetch_title_impl(&url))
		.await
		.map_err(|e| format!("下載執行緒失敗: {}", e))?
}

pub fn fetch_title_impl(url: &str) -> Result<TitleInfo, String> {
	let output = std::process::Command::new(yt_dlp_bin())
		.args(["--print", "title", "--no-download", url])
		.output()
		.map_err(|e| format!("無法執行 yt-dlp: {}", e))?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		log_error_line("download", &format!("無法取得標題: {} — {}", url, last_lines(&stderr, 3)));
		return Err("無法取得標題".to_string());
	}
	let title = String::from_utf8_lossy(&output.stdout).trim().to_string();
	if title.is_empty() {
		return Err("無法取得標題".to_string());
	}
	Ok(TitleInfo { folder: clean_title(&title), title })
}

/// 與 download.py 的 clean_title 同規則:去掉 '|' 之後、(...)、[...]、
/// 非「字母數字/底線/連字號/空白」字元,空白轉底線
fn clean_title(title: &str) -> String {
	let before_pipe = title.split('|').next().unwrap_or("");

	let mut stripped = String::new();
	let mut closing: Option<char> = None;
	for c in before_pipe.chars() {
		match closing {
			Some(close) => {
				if c == close {
					closing = None;
				}
			}
			None => match c {
				'(' => closing = Some(')'),
				'[' => closing = Some(']'),
				_ => stripped.push(c),
			},
		}
	}

	let filtered: String = stripped
		.chars()
		.filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || c.is_whitespace())
		.collect();
	filtered.split_whitespace().collect::<Vec<_>>().join("_")
}

#[tauri::command]
pub async fn download_audio(
	app: tauri::AppHandle,
	url: String,
	folder: Option<String>,
) -> Result<String, String> {
	tokio::task::spawn_blocking(move || {
		run_download(&url, folder, &|event, payload| {
			let _ = app.emit(event, payload);
		})
	})
	.await
	.map_err(|e| format!("下載執行緒失敗: {}", e))?
}

pub fn run_download(
	url: &str,
	folder: Option<String>,
	notify: &dyn Fn(&str, String),
) -> Result<String, String> {
	let start = Instant::now();
	log_info_line("download", &format!("開始下載: {}", url));

	// 前端流程會先 fetch_title 讓使用者確認資料夾名;沒給就自己取
	let (title, folder_name) = match folder.filter(|f| !f.trim().is_empty()) {
		Some(f) => {
			let f = f.trim().to_string();
			(f.clone(), f)
		}
		None => {
			let info = fetch_title_impl(url)?;
			(info.title, info.folder)
		}
	};

	// folder_name 可能來自使用者在 UI 手動編輯,同樣要擋路徑跳脫
	crate::validate_folder(&folder_name)?;
	let folder_path = data_dir().join("podcasts").join(&folder_name);
	if folder_path.exists() {
		log_error_line("download", &format!("資料夾已存在: {}", folder_name));
		return Err(format!("資料夾已存在:{}", folder_name));
	}
	std::fs::create_dir_all(&folder_path).map_err(|e| format!("建立資料夾失敗: {}", e))?;
	log_info_line("download", &format!("標題: {}, 資料夾: {}", title, folder_name));
	notify("download-title", title);

	let output_path = folder_path.join("podcast.mp3");
	let spawn_result = std::process::Command::new(yt_dlp_bin())
		.args([
			"-x",
			"--audio-format",
			"mp3",
			"--ffmpeg-location",
			"/opt/homebrew/bin",
			// android vr client 會被 YouTube 回 403(2026-07 實測),固定用 default client
			"--extractor-args",
			"youtube:player_client=default",
			"--newline",
			"-o",
		])
		.arg(&output_path)
		.arg(url)
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn();

	let mut child = match spawn_result {
		Ok(c) => c,
		Err(e) => {
			let _ = std::fs::remove_dir_all(&folder_path);
			return Err(format!("無法執行 yt-dlp: {}", e));
		}
	};

	// stderr 另開執行緒排空,避免管線塞滿造成死鎖
	let stderr = child.stderr.take().unwrap();
	let stderr_handle =
		std::thread::spawn(move || BufReader::new(stderr).lines().map_while(Result::ok).collect::<Vec<_>>());

	let mut tail: Vec<String> = Vec::new();
	for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
		if tail.len() >= 10 {
			tail.remove(0);
		}
		tail.push(line.clone());
		if let Some(pct) = parse_percent(&line) {
			notify("download-progress", pct);
		} else if line.contains("[ExtractAudio]") {
			notify("download-progress", "converting".to_string());
		}
	}

	let status = child.wait().map_err(|e| e.to_string())?;
	let stderr_lines = stderr_handle.join().unwrap_or_default();
	let elapsed = start.elapsed().as_secs_f32();

	if !status.success() {
		let detail = [tail, stderr_lines].concat().join("\n");
		log_error_line(
			"download",
			&format!("yt-dlp 下載失敗 (耗時 {:.1}s): {} — {}", elapsed, url, last_lines(&detail, 6)),
		);
		// 失敗時清掉這次建立的資料夾(含 .part 殘檔),避免重試撞「資料夾已存在」
		let _ = std::fs::remove_dir_all(&folder_path);
		return Err("下載失敗".to_string());
	}

	log_info_line("download", &format!("下載完成: {} (耗時 {:.1}s)", folder_name, elapsed));
	Ok(format!("podcasts/{}/podcast.mp3", folder_name))
}

/// 從 yt-dlp --newline 的輸出行抓進度,如 "[download]  45.3% of ..." → "45.3"
fn parse_percent(line: &str) -> Option<String> {
	for token in line.split_whitespace() {
		if let Some(num) = token.strip_suffix('%') {
			if num.parse::<f64>().is_ok() {
				return Some(num.to_string());
			}
		}
	}
	None
}

fn last_lines(text: &str, n: usize) -> String {
	let lines: Vec<&str> = text.lines().collect();
	lines[lines.len().saturating_sub(n)..].join(" / ")
}
