// headless 端對端測試:不開 GUI 直接跑原生轉譯管線
// 用法:cargo run --release --bin native_test <folder>

use std::sync::Arc;

fn main() {
	let folder = match std::env::args().nth(1) {
		Some(f) => f,
		None => {
			eprintln!("用法: native_test <podcasts 資料夾名>");
			std::process::exit(1);
		}
	};
	let progress: app_lib::native_transcribe::Progress =
		Arc::new(|msg: String| println!("PROGRESS: {}", msg));
	match app_lib::native_transcribe::run_pipeline(&folder, progress) {
		Ok(path) => println!("DONE: {}", path),
		Err(e) => {
			eprintln!("ERROR: {}", e);
			std::process::exit(1);
		}
	}
}
