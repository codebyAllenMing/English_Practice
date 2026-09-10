//! Tutor bot:依集隔離的 AI 助教。
//! context 組裝規則(docs/db-design.html Messages 區):word.txt 全文 + analysis.json 講義
//! + anchorLine 當前句 + 本集歷史對話(Messages 表);走與校正/講義同一套 API/CLI 雙模式。
//! 一問一答各寫一列 Messages——這就是 bot 的跨 session 記憶。

use crate::{load_config_merged, podcasts_dir, try_begin_task, validate_folder};

#[tauri::command]
pub async fn ask_tutor(folder: String, question: String, anchor_line: i32) -> Result<serde_json::Value, String> {
	validate_folder(&folder)?;
	let question = question.trim().to_string();
	if question.is_empty() {
		return Err("問題不可空白".to_string());
	}
	// 防重送:同集同時只跑一個提問
	let _guard = try_begin_task("提問", &folder)?;
	let start = std::time::Instant::now();
	let lang = crate::course_language();
	let dir = podcasts_dir().join(&folder);

	let transcript =
		std::fs::read_to_string(dir.join("word.txt")).map_err(|_| "找不到逐字稿,請先轉譯".to_string())?;
	let numbered = transcript
		.lines()
		.map(str::trim)
		.filter(|l| !l.is_empty())
		.enumerate()
		.map(|(i, l)| format!("{}|{}", i + 1, l))
		.collect::<Vec<_>>()
		.join("\n");
	let anchor_text = numbered
		.lines()
		.nth((anchor_line - 1).max(0) as usize)
		.unwrap_or("(超出範圍)")
		.to_string();

	// 講義有就夾帶(summary + 詞彙條列),沒有不擋提問
	let notes = std::fs::read_to_string(dir.join("analysis.json"))
		.ok()
		.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
		.map(|v| {
			let summary = v["result"]["summary"].as_str().unwrap_or("").to_string();
			let vocab = v["result"]["vocab"]
				.as_array()
				.map(|a| {
					a.iter()
						.map(|x| {
							format!(
								"- L{} {}: {}",
								x["line"],
								x["term"].as_str().unwrap_or(""),
								x["meaning"].as_str().unwrap_or("")
							)
						})
						.collect::<Vec<_>>()
						.join("\n")
				})
				.unwrap_or_default();
			format!("{}\n\nKey vocabulary:\n{}", summary, vocab)
		})
		.unwrap_or_else(|| "(尚未生成講義)".to_string());

	// 歷史對話:最近 20 列(10 輪),舊→新
	let history = crate::db::list_messages(folder.clone())?;
	let recent = if history.len() > 20 { &history[history.len() - 20..] } else { &history[..] };
	let history_txt = if recent.is_empty() {
		"(第一次提問)".to_string()
	} else {
		recent
			.iter()
			.map(|m| {
				let who = if m["role"] == "user" { "User" } else { "Tutor" };
				format!("{}: {}", who, m["content"].as_str().unwrap_or(""))
			})
			.collect::<Vec<_>>()
			.join("\n")
	};

	let prompt = format!(
		"You are a language-learning tutor inside a shadowing-practice app. The user is practicing \
		ONE podcast episode; everything you say must be grounded in this episode.\n\
		Answer IN TRADITIONAL CHINESE (zh-TW), in PLAIN TEXT (no markdown, no asterisks or headers). \
		Be concise and specific: explain usage as it appears \
		in THIS episode's context, cite line numbers like (L34) when referring to the transcript, \
		and avoid generic textbook lectures. If the question says 這句/這裡, it refers to the current line.\n\n\
		=== Episode transcript (1-based line numbers) ===\n{}\n\n\
		=== Study notes ===\n{}\n\n\
		=== Conversation so far ===\n{}\n\n\
		=== Current position ===\nLine {}: {}\n\n\
		=== Question ===\n{}",
		numbered, notes, history_txt, anchor_line, anchor_text, question
	);

	let config = load_config_merged();
	let mode = config["correction_mode"].as_str().unwrap_or("api").to_string();
	let answer = if mode == "cli" {
		crate::llm_via_cli(&prompt).await?
	} else {
		let api_key = config["anthropic_api_key"]
			.as_str()
			.filter(|k| !k.is_empty())
			.ok_or("請先在設定填入 Anthropic API Key(或切換為本機 Claude CLI 模式)")?;
		crate::llm_via_api(api_key, &prompt, None).await?
	};
	let answer = answer.trim().to_string();

	crate::db::insert_message(&lang, &folder, "user", &question, Some(anchor_line));
	crate::db::insert_message(&lang, &folder, "assistant", &answer, Some(anchor_line));
	crate::log_info_line(
		"tutor",
		&format!("[{}] 提問回答完成 (耗時 {:.1}s, mode {}, anchor L{})", folder, start.elapsed().as_secs_f32(), mode, anchor_line),
	);
	Ok(serde_json::json!({ "answer": answer }))
}
