//! 模型下載器:首次啟動(或缺檔)時把模型抓到 data_dir()/models。
//! 進度以 model-progress 事件回報:{name, received, total, index, count}。
//! tar.bz2 直接用 macOS 內建 /usr/bin/tar 解,不引入解壓 crate。

use std::io::Write;

use tauri::Emitter;

use crate::{data_dir, log_error_line, log_info_line, try_begin_task};

enum Kind {
	File,
	TarBz2,
}

struct ModelSpec {
	/// 顯示名稱(前端進度用)
	name: &'static str,
	url: &'static str,
	/// 完成判定檔(相對 models/);tar 解開後也以此驗證
	marker: &'static str,
	kind: Kind,
	/// 約略大小(bytes,伺服器沒回 content-length 時的進度分母)
	approx_bytes: u64,
}

const MODELS: [ModelSpec; 4] = [
	ModelSpec {
		name: "whisper 轉譯模型",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
		marker: "ggml-large-v3-turbo-q5_0.bin",
		kind: Kind::File,
		approx_bytes: 602_000_000,
	},
	ModelSpec {
		name: "講者分段模型",
		url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2",
		marker: "sherpa-onnx-pyannote-segmentation-3-0/model.onnx",
		kind: Kind::TarBz2,
		approx_bytes: 6_500_000,
	},
	ModelSpec {
		name: "講者聲紋模型",
		url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/nemo_en_titanet_small.onnx",
		marker: "nemo_en_titanet_small.onnx",
		kind: Kind::File,
		approx_bytes: 42_000_000,
	},
	ModelSpec {
		name: "kokoro 語音模型",
		url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-multi-lang-v1_0.tar.bz2",
		marker: "kokoro-multi-lang-v1_0/model.onnx",
		kind: Kind::TarBz2,
		approx_bytes: 349_000_000,
	},
];

fn missing_models() -> Vec<&'static ModelSpec> {
	let models_dir = data_dir().join("models");
	MODELS.iter().filter(|m| !models_dir.join(m.marker).exists()).collect()
}

/// 前端啟動時檢查:{ready, missing: [{name, mb}], totalMb}
#[tauri::command]
pub fn models_status() -> serde_json::Value {
	let missing = missing_models();
	serde_json::json!({
		"ready": missing.is_empty(),
		"missing": missing.iter().map(|m| serde_json::json!({
			"name": m.name,
			"mb": m.approx_bytes / 1_048_576,
		})).collect::<Vec<_>>(),
		"totalMb": missing.iter().map(|m| m.approx_bytes).sum::<u64>() / 1_048_576,
	})
}

#[tauri::command]
pub async fn download_models(app: tauri::AppHandle) -> Result<(), String> {
	let _guard = try_begin_task("模型下載", "models")?;
	download_models_impl(move |payload| {
		let _ = app.emit("model-progress", payload);
	})
	.await
}

pub async fn download_models_impl<F>(notify: F) -> Result<(), String>
where
	F: Fn(serde_json::Value) + Send + Sync,
{
	let start = std::time::Instant::now();
	let missing = missing_models();
	let count = missing.len();
	let models_dir = data_dir().join("models");
	std::fs::create_dir_all(&models_dir).map_err(|e| format!("建立 models 目錄失敗: {}", e))?;

	for (index, spec) in missing.iter().enumerate() {
		log_info_line("models", &format!("開始下載: {} ({})", spec.name, spec.url));

		let dest = match spec.kind {
			Kind::File => models_dir.join(spec.marker),
			Kind::TarBz2 => models_dir.join(".download.tmp.tar.bz2"),
		};
		if let Err(e) = fetch_to_file(spec, &dest, index, count, &notify).await {
			let _ = std::fs::remove_file(&dest);
			log_error_line("models", &format!("下載失敗: {} — {}", spec.name, e));
			return Err(format!("{} 下載失敗:{}", spec.name, e));
		}

		if let Kind::TarBz2 = spec.kind {
			let status = std::process::Command::new("/usr/bin/tar")
				.arg("xjf")
				.arg(&dest)
				.arg("-C")
				.arg(&models_dir)
				.status()
				.map_err(|e| format!("無法執行 tar: {}", e))?;
			let _ = std::fs::remove_file(&dest);
			if !status.success() {
				log_error_line("models", &format!("解壓失敗: {}", spec.name));
				return Err(format!("{} 解壓失敗", spec.name));
			}
		}

		if !models_dir.join(spec.marker).exists() {
			return Err(format!("{} 下載後驗證失敗(缺 {})", spec.name, spec.marker));
		}
		log_info_line("models", &format!("完成: {}", spec.name));
	}

	log_info_line(
		"models",
		&format!("全部模型就緒 (共 {} 項, 耗時 {:.0}s)", count, start.elapsed().as_secs_f32()),
	);
	Ok(())
}

async fn fetch_to_file<F>(
	spec: &ModelSpec,
	dest: &std::path::Path,
	index: usize,
	count: usize,
	notify: &F,
) -> Result<(), String>
where
	F: Fn(serde_json::Value) + Send + Sync,
{
	let mut resp = reqwest::get(spec.url).await.map_err(|e| format!("連線失敗: {}", e))?;
	if !resp.status().is_success() {
		return Err(format!("HTTP {}", resp.status().as_u16()));
	}
	let total = resp.content_length().unwrap_or(spec.approx_bytes);

	let mut file = std::fs::File::create(dest).map_err(|e| format!("建立檔案失敗: {}", e))?;
	let mut received: u64 = 0;
	let mut last_emit: u64 = 0;
	while let Some(chunk) = resp.chunk().await.map_err(|e| format!("下載中斷: {}", e))? {
		file.write_all(&chunk).map_err(|e| format!("寫檔失敗: {}", e))?;
		received += chunk.len() as u64;
		// 每 3MB 回報一次,避免事件洪水
		if received - last_emit >= 3_000_000 || received == total {
			last_emit = received;
			notify(serde_json::json!({
				"name": spec.name,
				"received": received,
				"total": total,
				"index": index,
				"count": count,
			}));
		}
	}
	Ok(())
}
