//! 分析講義:校正完成後對整集逐字稿做一次性 AI 分析——大綱(分段錨定行號)
//! 與詞彙精講(語境解釋、出處行號)。產物 analysis.json 落集資料夾,終身快取,
//! 之後進練習頁純讀檔零 AI 呼叫;走與校正相同的 API/CLI 雙模式。

use std::fs;

use crate::{
	load_config_merged, log_error_line, log_info_line, podcasts_dir, try_begin_task, validate_folder,
};

#[tauri::command]
pub async fn analyze_transcript(folder: String) -> Result<serde_json::Value, String> {
	validate_folder(&folder)?;
	let _guard = try_begin_task("分析", &folder)?;
	let start = std::time::Instant::now();
	log_info_line("analysis", &format!("開始分析: {}", folder));
	match analyze_inner(&folder).await {
		Ok(res) => {
			log_info_line(
				"analysis",
				&format!(
					"[{}] 講義完成 (耗時 {:.1}s, mode {}, 大綱 {} 段, 詞彙 {} 則)",
					folder,
					start.elapsed().as_secs_f32(),
					res["mode"].as_str().unwrap_or("?"),
					res["sections"],
					res["vocab"],
				),
			);
			Ok(res)
		}
		Err(e) => {
			log_error_line(
				"analysis",
				&format!("[{}] 分析失敗 (耗時 {:.1}s): {}", folder, start.elapsed().as_secs_f32(), e),
			);
			Err(e)
		}
	}
}

async fn analyze_inner(folder: &str) -> Result<serde_json::Value, String> {
	let folder_path = podcasts_dir().join(folder);
	let word_path = folder_path.join("word.txt");
	if !word_path.exists() {
		return Err("找不到逐字稿,請先轉譯".to_string());
	}
	let text = fs::read_to_string(&word_path).map_err(|e| format!("讀取 word.txt 失敗: {}", e))?;
	let numbered = text
		.lines()
		.filter(|l| !l.trim().is_empty())
		.enumerate()
		.map(|(i, l)| format!("{}|{}", i + 1, l))
		.collect::<Vec<_>>()
		.join("\n");
	let total_lines = numbered.lines().count();

	// 詞彙挑選基準依課綱語系(討論定案:中級視角、每集 10~20 則)
	let (level_note, reading_note) = if crate::course_language() == "ja" {
		("JLPT N4-N3 level Japanese learner", "the hiragana reading of the term")
	} else {
		("CEFR B1-B2 level English learner", "an empty string")
	};

	let prompt = format!(
		"Below is a podcast transcript used for language-learning shadowing practice. \
		Each line is prefixed with its 1-based line number followed by '|'. \
		Lines have the format \"[Speaker]: text\".\n\n\
		Produce study notes IN TRADITIONAL CHINESE (zh-TW) with three parts:\n\
		1. summary: 2-3 sentences describing what this episode is about and how the conversation unfolds.\n\
		2. sections: 3-8 outline segments covering the whole transcript in order. Each has a short zh-TW title, \
		startLine and endLine (1-based). Segments must be in order, non-overlapping, and together cover \
		line 1 through line {}.\n\
		3. vocab: 10-20 items genuinely worth studying for a {} — idioms, collocations, \
		phrasal expressions, grammar patterns, honorifics/register. Skip trivial basics and rare obscure words. \
		Each item has: term (exactly as it appears in the transcript), reading ({}), \
		type (a short zh-TW tag such as 慣用/文法/搭配/片語/敬語), \
		meaning (a zh-TW explanation of how it is used IN THIS episode's context — not a generic dictionary gloss), \
		line (the 1-based line number where it appears), quote (the exact sentence or fragment from that line).\n\n\
		Respond with ONLY a JSON object of the shape \
		{{\"summary\": \"...\", \"sections\": [{{\"title\": \"...\", \"startLine\": 1, \"endLine\": 20}}], \
		\"vocab\": [{{\"term\": \"...\", \"reading\": \"...\", \"type\": \"...\", \"meaning\": \"...\", \
		\"line\": 34, \"quote\": \"...\"}}]}} — no markdown fences, no explanations.\n\n\
		Transcript:\n{}",
		total_lines, level_note, reading_note, numbered
	);

	let config = load_config_merged();
	let mode = config["correction_mode"].as_str().unwrap_or("api").to_string();
	let result_text = if mode == "cli" {
		crate::llm_via_cli(&prompt).await?
	} else {
		let api_key = config["anthropic_api_key"]
			.as_str()
			.filter(|k| !k.is_empty())
			.ok_or("請先在設定填入 Anthropic API Key(或切換為本機 Claude CLI 模式)")?;
		crate::llm_via_api(api_key, &prompt, analysis_schema()).await?
	};

	let parsed: serde_json::Value = serde_json::from_str(crate::extract_json(&result_text))
		.map_err(|e| format!("解析講義結果失敗: {}", e))?;
	// 最低限度形狀驗證:三個欄位都要在,行號錨定是這頁的靈魂,缺了就當失敗重跑
	let sections = parsed["sections"].as_array().ok_or("講義結果缺 sections")?.len();
	let vocab = parsed["vocab"].as_array().ok_or("講義結果缺 vocab")?.len();
	if parsed["summary"].as_str().unwrap_or("").is_empty() {
		return Err("講義結果缺 summary".to_string());
	}

	let record = serde_json::json!({
		"mode": mode,
		"generatedAt": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
		"result": parsed,
	});
	fs::write(
		folder_path.join("analysis.json"),
		serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?,
	)
	.map_err(|e| format!("寫入 analysis.json 失敗: {}", e))?;

	Ok(serde_json::json!({ "sections": sections, "vocab": vocab, "mode": mode }))
}

fn analysis_schema() -> serde_json::Value {
	serde_json::json!({
		"type": "object",
		"properties": {
			"summary": { "type": "string" },
			"sections": {
				"type": "array",
				"items": {
					"type": "object",
					"properties": {
						"title": { "type": "string" },
						"startLine": { "type": "integer" },
						"endLine": { "type": "integer" }
					},
					"required": ["title", "startLine", "endLine"],
					"additionalProperties": false
				}
			},
			"vocab": {
				"type": "array",
				"items": {
					"type": "object",
					"properties": {
						"term": { "type": "string" },
						"reading": { "type": "string" },
						"type": { "type": "string" },
						"meaning": { "type": "string" },
						"line": { "type": "integer" },
						"quote": { "type": "string" }
					},
					"required": ["term", "reading", "type", "meaning", "line", "quote"],
					"additionalProperties": false
				}
			}
		},
		"required": ["summary", "sections", "vocab"],
		"additionalProperties": false
	})
}

/// 讀取快取的講義;不存在回 {exists: false}(前端據此顯示「生成講義」)
#[tauri::command]
pub fn get_analysis(folder: String) -> Result<serde_json::Value, String> {
	validate_folder(&folder)?;
	let path = podcasts_dir().join(&folder).join("analysis.json");
	if !path.exists() {
		return Ok(serde_json::json!({ "exists": false }));
	}
	let record: serde_json::Value = serde_json::from_str(
		&fs::read_to_string(&path).map_err(|e| format!("讀取 analysis.json 失敗: {}", e))?,
	)
	.map_err(|e| format!("解析 analysis.json 失敗: {}", e))?;
	Ok(serde_json::json!({ "exists": true, "result": record["result"] }))
}
