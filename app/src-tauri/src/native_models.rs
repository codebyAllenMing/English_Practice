//! 模型下載器:首次啟動(或缺檔)時把模型抓到 data_dir()/models。
//! 進度以 model-progress 事件回報:{name, received, total, index, count}。
//! tar.bz2 直接用 macOS 內建 /usr/bin/tar 解,不引入解壓 crate。

use std::io::Write;

use tauri::Emitter;

use crate::{data_dir, log_error_line, log_info_line, try_begin_task};

enum Kind {
	File,
	TarBz2,
	/// zip(VOICEVOX 的 .vvpp);解到 marker 的父目錄下,macOS 內建 /usr/bin/unzip
	Zip,
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
	/// marker 檔的 SHA256(完整性驗證;自官方源下載的正本計得,2026-07-28)
	sha256: &'static str,
}

const MODELS: [ModelSpec; 4] = [
	ModelSpec {
		name: "whisper 轉譯模型",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
		marker: "ggml-large-v3-turbo-q5_0.bin",
		kind: Kind::File,
		approx_bytes: 602_000_000,
		sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
	},
	ModelSpec {
		name: "講者分段模型",
		url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2",
		marker: "sherpa-onnx-pyannote-segmentation-3-0/model.onnx",
		kind: Kind::TarBz2,
		approx_bytes: 6_500_000,
		sha256: "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079",
	},
	ModelSpec {
		name: "講者聲紋模型",
		url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/nemo_en_titanet_small.onnx",
		marker: "nemo_en_titanet_small.onnx",
		kind: Kind::File,
		approx_bytes: 42_000_000,
		sha256: "ad4a1802485d8b34c722d2a9d04249662f2ece5d28a7a039063ca22f515a789e",
	},
	ModelSpec {
		name: "kokoro 語音模型",
		url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-multi-lang-v1_0.tar.bz2",
		marker: "kokoro-multi-lang-v1_0/model.onnx",
		kind: Kind::TarBz2,
		approx_bytes: 349_000_000,
		sha256: "c436dc6a842b62aba06af67e40bafcfb9c60ac3af895358f1974ad9a7f7c026b",
	},
];

/// 日文語音引擎:不在 MODELS 首次下載清單(英文使用者不強迫吃 1.8GB),
/// 由練習頁在日文課綱且缺引擎時按需觸發 download_voicevox
const VOICEVOX_ENGINE: ModelSpec = ModelSpec {
	name: "VOICEVOX 日文語音引擎",
	url: "https://github.com/VOICEVOX/voicevox_engine/releases/download/0.25.2/voicevox_engine-macos-arm64-0.25.2.vvpp",
	marker: "voicevox_engine/run",
	kind: Kind::Zip,
	approx_bytes: 1_887_128_088,
	sha256: "b4db0626f90bca175f4a1833394410f7abd263d2d85fdaa64100861181dcdea5",
};

#[tauri::command]
pub fn voicevox_status() -> serde_json::Value {
	serde_json::json!({
		"ready": crate::native_voicevox::engine_installed(),
		"mb": VOICEVOX_ENGINE.approx_bytes / 1_048_576,
	})
}

#[tauri::command]
pub async fn download_voicevox(app: tauri::AppHandle) -> Result<(), String> {
	let _guard = try_begin_task("模型下載", "voicevox")?;
	let models_dir = data_dir().join("models");
	std::fs::create_dir_all(&models_dir).map_err(|e| format!("建立 models 目錄失敗: {}", e))?;
	download_one(&VOICEVOX_ENGINE, &models_dir, 0, 1, &move |payload| {
		let _ = app.emit("model-progress", payload);
	})
	.await
}

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
		download_one(spec, &models_dir, index, count, &notify).await?;
	}

	log_info_line(
		"models",
		&format!("全部模型就緒 (共 {} 項, 耗時 {:.0}s)", count, start.elapsed().as_secs_f32()),
	);
	Ok(())
}

/// 下載單一模型:抓檔 → 解壓(tar/zip)→ marker 存在性 + SHA256 驗證
async fn download_one<F>(
	spec: &ModelSpec,
	models_dir: &std::path::Path,
	index: usize,
	count: usize,
	notify: &F,
) -> Result<(), String>
where
	F: Fn(serde_json::Value) + Send + Sync,
{
	log_info_line("models", &format!("開始下載: {} ({})", spec.name, spec.url));

	let dest = match spec.kind {
		Kind::File => models_dir.join(spec.marker),
		Kind::TarBz2 => models_dir.join(".download.tmp.tar.bz2"),
		Kind::Zip => models_dir.join(".download.tmp.zip"),
	};
	if let Err(e) = fetch_to_file(spec, &dest, index, count, notify).await {
		let _ = std::fs::remove_file(&dest);
		log_error_line("models", &format!("下載失敗: {} — {}", spec.name, e));
		return Err(format!("{} 下載失敗:{}", spec.name, e));
	}

	match spec.kind {
		Kind::File => {}
		Kind::TarBz2 => {
			let status = std::process::Command::new("/usr/bin/tar")
				.arg("xjf")
				.arg(&dest)
				.arg("-C")
				.arg(models_dir)
				.status()
				.map_err(|e| format!("無法執行 tar: {}", e))?;
			let _ = std::fs::remove_file(&dest);
			if !status.success() {
				log_error_line("models", &format!("解壓失敗: {}", spec.name));
				return Err(format!("{} 解壓失敗", spec.name));
			}
		}
		Kind::Zip => {
			// zip 內容在封存根層,解到 marker 的父目錄(如 voicevox_engine/)
			let target = models_dir.join(std::path::Path::new(spec.marker).parent().unwrap_or_else(|| "".as_ref()));
			let status = std::process::Command::new("/usr/bin/unzip")
				.arg("-oq")
				.arg(&dest)
				.arg("-d")
				.arg(&target)
				.status()
				.map_err(|e| format!("無法執行 unzip: {}", e))?;
			let _ = std::fs::remove_file(&dest);
			if !status.success() {
				log_error_line("models", &format!("解壓失敗: {}", spec.name));
				return Err(format!("{} 解壓失敗", spec.name));
			}
		}
	}

	let marker_path = models_dir.join(spec.marker);
	if !marker_path.exists() {
		return Err(format!("{} 下載後驗證失敗(缺 {})", spec.name, spec.marker));
	}
	// 完整性驗證:hash 不符即清除,避免留下毒檔
	let actual = sha256_file(&marker_path)?;
	if actual != spec.sha256 {
		remove_model_artifact(models_dir, spec.marker);
		log_error_line(
			"models",
			&format!("SHA256 不符: {} (expected {}, got {})", spec.name, spec.sha256, actual),
		);
		return Err(format!("{} 完整性驗證失敗,已刪除,請重試", spec.name));
	}
	// zip 不保證還原執行權限,marker 為執行檔時補上
	if let Kind::Zip = spec.kind {
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let _ = std::fs::set_permissions(&marker_path, std::fs::Permissions::from_mode(0o755));
		}
	}
	log_info_line("models", &format!("完成: {} (SHA256 驗證通過)", spec.name));
	Ok(())
}

fn sha256_file(path: &std::path::Path) -> Result<String, String> {
	use sha2::{Digest, Sha256};
	use std::io::Read;
	let mut file = std::fs::File::open(path).map_err(|e| format!("讀檔失敗: {}", e))?;
	let mut hasher = Sha256::new();
	let mut buf = vec![0u8; 4 * 1024 * 1024];
	loop {
		let n = file.read(&mut buf).map_err(|e| format!("讀檔失敗: {}", e))?;
		if n == 0 {
			break;
		}
		hasher.update(&buf[..n]);
	}
	Ok(format!("{:x}", hasher.finalize()))
}

/// 清除某個模型的落地物:單檔直接刪;tar 解出來的整個目錄一起刪
fn remove_model_artifact(models_dir: &std::path::Path, marker: &str) {
	match std::path::Path::new(marker).parent() {
		Some(parent) if parent != std::path::Path::new("") => {
			let _ = std::fs::remove_dir_all(models_dir.join(parent));
		}
		_ => {
			let _ = std::fs::remove_file(models_dir.join(marker));
		}
	}
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
