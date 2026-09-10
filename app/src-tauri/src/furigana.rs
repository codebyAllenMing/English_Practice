//! 振り仮名(furigana):lindera + 內嵌 IPADIC 詞典,詞級標音。
//! 只對含漢字的詞附讀音(平假名);純假名、標點、查無讀音的詞原樣輸出。
//! 詞典編進 binary(embed-ipadic),零下載零外部依賴;首次呼叫時延遲載入。

use std::borrow::Cow;
use std::sync::OnceLock;

use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;

fn segmenter() -> Option<&'static Segmenter> {
	static SEG: OnceLock<Option<Segmenter>> = OnceLock::new();
	SEG.get_or_init(|| match load_dictionary("embedded://ipadic") {
		Ok(dict) => Some(Segmenter::new(Mode::Normal, dict, None)),
		Err(e) => {
			crate::log_error_line("furigana", &format!("IPADIC 載入失敗: {}", e));
			None
		}
	})
	.as_ref()
}

/// 一行內文 → [(詞面, 讀音)];讀音只在詞含漢字時給(平假名),其餘為 None。
/// 斷詞失敗時整行原樣回傳(不標音),絕不擋住顯示
pub fn annotate(text: &str) -> Vec<(String, Option<String>)> {
	let Some(seg) = segmenter() else {
		return vec![(text.to_string(), None)];
	};
	let Ok(mut tokens) = seg.segment(Cow::Borrowed(text)) else {
		return vec![(text.to_string(), None)];
	};
	let mut out = Vec::with_capacity(tokens.len());
	for token in tokens.iter_mut() {
		let surface = token.surface.to_string();
		let reading = if surface.chars().any(is_kanji) {
			// IPADIC details:[品詞, 細分1, 細分2, 細分3, 活用型, 活用形, 原形, 読み, 発音]
			token
				.details()
				.get(7)
				.map(|r| kata_to_hira(r))
				.filter(|r| !r.is_empty() && r != "*")
		} else {
			None
		};
		out.push((surface, reading));
	}
	out
}

fn is_kanji(c: char) -> bool {
	matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '々')
}

/// 片假名 → 平假名(IPADIC 讀音欄為片假名)
fn kata_to_hira(s: &str) -> String {
	s.chars()
		.map(|c| match c {
			'\u{30A1}'..='\u{30F6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
			_ => c,
		})
		.collect()
}

/// 詞性即查(本地 lindera,ja 限定;en 回 null)——AI 釋義到達前先顯示,零延遲
#[tauri::command]
pub fn term_pos(term: String) -> Option<String> {
	if crate::course_language() != "ja" {
		return None;
	}
	let seg = segmenter()?;
	let mut tokens = seg.segment(Cow::Borrowed(term.as_str())).ok()?;
	let first = tokens.first_mut()?;
	first.details().first().filter(|s| **s != "UNK").map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
	#[test]
	fn annotate_reads_kanji_words() {
		let segs = super::annotate("語学の教科書を読み込んで");
		// 詞面拼回原文(斷詞不掉字)
		let joined: String = segs.iter().map(|(s, _)| s.as_str()).collect();
		assert_eq!(joined, "語学の教科書を読み込んで");
		// 含漢字詞有平假名讀音;純假名詞(の/を)沒有
		let readings: Vec<String> = segs.iter().filter_map(|(_, r)| r.clone()).collect();
		assert!(readings.contains(&"ごがく".to_string()), "{:?}", segs);
		assert!(segs.iter().any(|(s, r)| s == "の" && r.is_none()));
	}
}

/// 閱讀模式用:word.txt 每行內文(去講者標籤)的標音,行序與 get_lines 對齊。
/// 非日文課綱回空陣列(前端原樣渲染)
#[tauri::command]
pub fn get_ruby(folder: String) -> Result<Vec<Vec<(String, Option<String>)>>, String> {
	crate::validate_folder(&folder)?;
	if crate::course_language() != "ja" {
		return Ok(vec![]);
	}
	let word_path = crate::podcasts_dir().join(&folder).join("word.txt");
	let content = std::fs::read_to_string(&word_path).map_err(|e| format!("讀取 word.txt 失敗: {}", e))?;
	Ok(content
		.lines()
		.map(|l| l.trim())
		.filter(|l| !l.is_empty())
		.map(|line| {
			let text = line.split_once("]:").map(|(_, t)| t.trim()).unwrap_or(line);
			annotate(text)
		})
		.collect())
}
