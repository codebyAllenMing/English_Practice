//! Tutor bot:依集隔離的 AI 助教。
//! context 組裝規則(docs/db-design.html Messages 區):word.txt 全文 + analysis.json 講義
//! + anchorLine 當前句 + 本集歷史對話(Messages 表);走與校正/講義同一套 API/CLI 雙模式。
//! 一問一答各寫一列 Messages——這就是 bot 的跨 session 記憶。

use crate::{load_config_merged, podcasts_dir, try_begin_task, validate_folder};

/// 點生字時的即查:詞性 + 語境化繁中釋義(AI 一次、寫回 VocabItems 終身快取)
#[tauri::command]
pub async fn lookup_term(folder: String, term: String, line_no: i32) -> Result<serde_json::Value, String> {
	validate_folder(&folder)?;
	let term = term.trim().to_string();
	if term.is_empty() {
		return Err("空白詞".to_string());
	}
	let lang = crate::course_language();
	if let Some(cached) = crate::db::get_lookup(&lang, &folder, line_no, &term) {
		return Ok(serde_json::json!({ "combined": cached, "cached": true }));
	}
	// 出處句當語境,解釋「在這句裡」的用法而非字典泛解
	let sentence = crate::native_tts::read_line(&folder, line_no - 1).map(|(_, t, _)| t).unwrap_or_default();
	let lang_name = if lang == "ja" { "Japanese" } else { "English" };
	let prompt = format!(
		"Term: {}\nSentence: {}\n\nThe term is {} as used in the sentence above. \
		Respond with ONLY a JSON object {{\"pos\": \"...\", \"meaning\": \"...\"}} — \
		pos is the part of speech in Traditional Chinese (e.g. 名詞/動詞/形容詞/副詞/慣用語/文法), \
		meaning is ONE concise Traditional Chinese sentence explaining the term AS USED in this sentence. \
		No markdown, no explanations outside the JSON.",
		term, sentence, lang_name
	);
	let config = load_config_merged();
	let mode = config["correction_mode"].as_str().unwrap_or("api").to_string();
	// CLI 偶發非 JSON 輸出(實測過),點詞是高頻互動,失敗自動重試一次
	let mut last_err = "查詢失敗".to_string();
	for _ in 0..2 {
		let result_text = if mode == "cli" {
			crate::llm_via_cli(&prompt).await?
		} else {
			let api_key = config["anthropic_api_key"]
				.as_str()
				.filter(|k| !k.is_empty())
				.ok_or("請先在設定填入 Anthropic API Key(或切換為本機 Claude CLI 模式)")?;
			let schema = serde_json::json!({
				"type": "object",
				"properties": { "pos": { "type": "string" }, "meaning": { "type": "string" } },
				"required": ["pos", "meaning"],
				"additionalProperties": false
			});
			crate::llm_via_api(api_key, &prompt, Some(schema)).await?
		};
		match serde_json::from_str::<serde_json::Value>(crate::extract_json(&result_text)) {
			Ok(parsed) => {
				let pos = parsed["pos"].as_str().unwrap_or("").to_string();
				let meaning = parsed["meaning"].as_str().unwrap_or("").to_string();
				if meaning.is_empty() {
					last_err = "查詢結果為空".to_string();
					continue;
				}
				let combined =
					if pos.is_empty() { meaning.clone() } else { format!("【{}】{}", pos, meaning) };
				crate::db::put_lookup(&lang, &folder, line_no, &term, &combined);
				// 已收藏但釋義空白的列順手補上
				crate::db::set_vocab_meaning(&lang, &folder, line_no, &term, &combined);
				return Ok(serde_json::json!({ "combined": combined, "cached": false }));
			}
			Err(e) => last_err = format!("解析查詢結果失敗: {}", e),
		}
	}
	Err(last_err)
}

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
