// headless 端對端測試:不開 GUI 直接跑原生管線
// 轉譯:cargo run --release --bin native_test <folder>
// TTS: cargo run --release --bin native_test tts <folder> <line-index> <out.wav>

use std::sync::Arc;

fn main() {
	let args: Vec<String> = std::env::args().collect();
	// 測試工具以專案根為資料目錄(release build 的預設是 Application Support);
	// models 模式可用 args[2] 指定其他目錄模擬「缺模型」情境,須早於此預設
	if args.get(1).map(|s| s.as_str()) == Some("models") {
		if let Some(dir) = args.get(2) {
			app_lib::init_data_dir(std::path::PathBuf::from(dir));
		}
	}
	app_lib::init_data_dir(app_lib::project_dir());
	match args.get(1).map(|s| s.as_str()) {
		Some("tts") => run_tts(&args),
		Some("sid") => run_sid(&args),
		Some("title") => run_title(&args),
		Some("download") => run_download(&args),
		Some("models") => run_models(&args),
		Some(folder) => run_transcribe(folder),
		None => {
			eprintln!("用法: native_test <folder> | tts <folder> <line-index> <out.wav> | sid <sid> <out.wav> | title <url> | download <url> <folder> | models [data-dir]");
			std::process::exit(1);
		}
	}
}

// 模型下載器測試(data dir 已於 main 設定)
fn run_models(_args: &[String]) {
	println!("status: {}", app_lib::native_models::models_status());
	let rt = tokio::runtime::Runtime::new().expect("建立 runtime 失敗");
	let result = rt.block_on(app_lib::native_models::download_models_impl(|p| {
		println!("progress: {}", p);
	}));
	match result {
		Ok(()) => println!("DONE: {}", app_lib::native_models::models_status()),
		Err(e) => {
			eprintln!("ERROR: {}", e);
			std::process::exit(1);
		}
	}
}

fn run_title(args: &[String]) {
	let url = args.get(2).expect("需要 url");
	match app_lib::native_download::fetch_title_impl(url) {
		Ok(info) => println!("TITLE: {}\nFOLDER: {}", info.title, info.folder),
		Err(e) => {
			eprintln!("ERROR: {}", e);
			std::process::exit(1);
		}
	}
}

fn run_download(args: &[String]) {
	let url = args.get(2).expect("需要 url");
	let folder = args.get(3).cloned();
	match app_lib::native_download::run_download(url, folder, &|event, payload| {
		println!("{}: {}", event, payload);
	}) {
		Ok(path) => println!("DONE: {}", path),
		Err(e) => {
			eprintln!("ERROR: {}", e);
			std::process::exit(1);
		}
	}
}

// 驗證 sid ↔ voice 對照表:固定文本、指定 sid 合成
fn run_sid(args: &[String]) {
	let (sid, out) = match (args.get(2), args.get(3)) {
		(Some(s), Some(o)) => (s.parse::<i32>().expect("sid 需為整數"), o),
		_ => {
			eprintln!("用法: native_test sid <sid> <out.wav>");
			std::process::exit(1);
		}
	};
	let engine = app_lib::native_tts::load_engine().expect("載入引擎失敗");
	let (samples, rate) = engine
		.synth_with_sid("Hello, welcome back to the learning lab. This is a voice test.", sid)
		.expect("合成失敗");
	assert!(sherpa_onnx::write(out, &samples, rate), "寫 wav 失敗");
	println!("sid={} → {}({} samples @ {} Hz)", sid, out, samples.len(), rate);
}

fn run_transcribe(folder: &str) {
	let progress: app_lib::native_transcribe::Progress =
		Arc::new(|msg: String| println!("PROGRESS: {}", msg));
	match app_lib::native_transcribe::run_pipeline(folder, progress) {
		Ok(path) => println!("DONE: {}", path),
		Err(e) => {
			eprintln!("ERROR: {}", e);
			std::process::exit(1);
		}
	}
}

fn run_tts(args: &[String]) {
	let (folder, index, out) = match (args.get(2), args.get(3), args.get(4)) {
		(Some(f), Some(i), Some(o)) => (f, i.parse::<i32>().expect("index 需為整數"), o),
		_ => {
			eprintln!("用法: native_test tts <folder> <line-index> <out.wav>");
			std::process::exit(1);
		}
	};

	let start = std::time::Instant::now();
	let mut engine = app_lib::native_tts::load_engine().unwrap_or_else(|e| {
		eprintln!("ERROR: {}", e);
		std::process::exit(1);
	});
	println!("模型載入: {:.1}s", start.elapsed().as_secs_f32());

	let start = std::time::Instant::now();
	match app_lib::native_tts::synth_line(&mut engine, folder, index) {
		Ok(result) => {
			let b64 = result["audio"].as_str().unwrap();
			let wav = {
				use base64::{engine::general_purpose::STANDARD, Engine as _};
				STANDARD.decode(b64).expect("base64 解碼失敗")
			};
			std::fs::write(out, &wav).expect("寫檔失敗");
			println!(
				"合成: {:.1}s  speaker={}  text={}  → {}({} bytes)",
				start.elapsed().as_secs_f32(),
				result["speaker"],
				result["text"],
				out,
				wav.len()
			);
		}
		Err(e) => {
			eprintln!("ERROR: {}", e);
			std::process::exit(1);
		}
	}
}
