//! SQLite 資料層:索引與行為(檔案仍是內容的唯一真相)。
//! 設計文件 docs/db-design.html:derived 表(Episodes/Lines/LinesFts)可掃 podcasts/ 重建;
//! primary 表(VocabItems/Messages/PracticeSessions/LineStats)是使用者資產,重建流程不碰,
//! 並以 (language, folder) 自然鍵指回集數(Episodes 重建後 id 改變也不斷鏈)。

use std::sync::{Mutex, MutexGuard, OnceLock};

use rusqlite::Connection;

use crate::{data_dir, log_error_line, log_info_line};

fn now() -> String {
	chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// 全域連線(WAL 單機單人足矣);首次存取時開檔+遷移
fn conn() -> MutexGuard<'static, Connection> {
	static DB: OnceLock<Mutex<Connection>> = OnceLock::new();
	DB.get_or_init(|| {
		let c = Connection::open(data_dir().join("app.db")).expect("開啟 app.db 失敗");
		let _ = c.pragma_update(None, "journal_mode", "WAL");
		let _ = c.pragma_update(None, "foreign_keys", "ON");
		migrate(&c);
		Mutex::new(c)
	})
	.lock()
	.expect("db lock poisoned")
}

/// PRAGMA user_version 驅動的遷移(加法優先;每個未來功能 = 版本 N+1)
fn migrate(c: &Connection) {
	let v: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap_or(0);
	if v < 1 {
		let result = c.execute_batch(
			r#"
			CREATE TABLE Episodes (
				id INTEGER PRIMARY KEY,
				language TEXT NOT NULL,
				folder TEXT NOT NULL,
				title TEXT,
				sourceUrl TEXT,
				status TEXT NOT NULL DEFAULT 'downloaded',
				durationSec REAL,
				speakers TEXT,
				createDate TEXT NOT NULL,
				updateDate TEXT NOT NULL,
				UNIQUE(language, folder)
			);
			CREATE TABLE Lines (
				id INTEGER PRIMARY KEY,
				episodeId INTEGER NOT NULL REFERENCES Episodes(id) ON DELETE CASCADE,
				lineNo INTEGER NOT NULL,
				speaker TEXT,
				content TEXT NOT NULL,
				UNIQUE(episodeId, lineNo)
			);
			CREATE VIRTUAL TABLE LinesFts USING fts5(content, content='Lines', content_rowid='id', tokenize='trigram');
			CREATE TRIGGER LinesAi AFTER INSERT ON Lines BEGIN
				INSERT INTO LinesFts(rowid, content) VALUES (new.id, new.content);
			END;
			CREATE TRIGGER LinesAd AFTER DELETE ON Lines BEGIN
				INSERT INTO LinesFts(LinesFts, rowid, content) VALUES ('delete', old.id, old.content);
			END;
			CREATE TABLE VocabItems (
				id INTEGER PRIMARY KEY,
				language TEXT NOT NULL,
				folder TEXT NOT NULL,
				lineNo INTEGER,
				term TEXT NOT NULL,
				reading TEXT,
				meaning TEXT,
				source TEXT NOT NULL DEFAULT 'user',
				synced INTEGER NOT NULL DEFAULT 0,
				addedDate TEXT NOT NULL,
				UNIQUE(language, folder, lineNo, term)
			);
			CREATE TABLE Messages (
				id INTEGER PRIMARY KEY,
				language TEXT NOT NULL,
				folder TEXT NOT NULL,
				role TEXT NOT NULL,
				content TEXT NOT NULL,
				anchorLine INTEGER,
				createDate TEXT NOT NULL
			);
			CREATE INDEX MessagesByEpisode ON Messages(language, folder, id);
			CREATE TABLE PracticeSessions (
				id INTEGER PRIMARY KEY,
				language TEXT NOT NULL,
				folder TEXT NOT NULL,
				startDate TEXT NOT NULL,
				endDate TEXT,
				linesPlayed INTEGER NOT NULL DEFAULT 0
			);
			CREATE TABLE LineStats (
				language TEXT NOT NULL,
				folder TEXT NOT NULL,
				lineNo INTEGER NOT NULL,
				playCount INTEGER NOT NULL DEFAULT 0,
				lastPlayed TEXT,
				PRIMARY KEY (language, folder, lineNo)
			);
			PRAGMA user_version = 1;
			"#,
		);
		match result {
			Ok(()) => log_info_line("db", "migration v1 完成(七表 + FTS5)"),
			Err(e) => log_error_line("db", &format!("migration v1 失敗: {}", e)),
		}
	}
}

// ── derived 表維護(掃描重建/單集刷新)──

/// 全量重建索引:掃 podcasts/<lang>/ 刷每集,並清掉資料夾已不存在的集數。app 啟動時跑
pub fn rebuild_index() {
	let start = std::time::Instant::now();
	let mut count = 0;
	for lang in ["en", "ja"] {
		let root = data_dir().join("podcasts").join(lang);
		let Ok(entries) = std::fs::read_dir(&root) else { continue };
		for e in entries.flatten() {
			if e.path().is_dir() {
				refresh_episode(lang, &e.file_name().to_string_lossy());
				count += 1;
			}
		}
	}
	// 移除孤兒集(資料夾已刪但列還在)
	let stale: Vec<(String, String)> = {
		let c = conn();
		let mut stmt = match c.prepare("SELECT language, folder FROM Episodes") {
			Ok(s) => s,
			Err(_) => return,
		};
		let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
		rows.map(|it| it.flatten().collect()).unwrap_or_default()
	};
	for (lang, folder) in stale {
		if !data_dir().join("podcasts").join(&lang).join(&folder).is_dir() {
			delete_episode(&lang, &folder);
		}
	}
	log_info_line("db", &format!("索引重建完成({} 集, {:.2}s)", count, start.elapsed().as_secs_f32()));
}

/// 單集刷新:依檔案推導 status/speakers、重灌 Lines(word.txt 為真相)
pub fn refresh_episode(language: &str, folder: &str) {
	let dir = data_dir().join("podcasts").join(language).join(folder);
	let status = if dir.join("analysis.json").exists() {
		"analyzed"
	} else if dir.join("correction.json").exists() {
		"corrected"
	} else if dir.join("word.txt").exists() {
		"transcribed"
	} else {
		"downloaded"
	};
	let speakers: Option<String> = std::fs::read_to_string(dir.join("correction.json"))
		.ok()
		.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
		.and_then(|v| {
			let arr = v["result"]["speakers"].as_array()?;
			let mut names: Vec<String> =
				arr.iter().filter_map(|s| s["to"].as_str().map(String::from)).collect();
			names.dedup();
			serde_json::to_string(&names).ok()
		});

	let c = conn();
	let ts = now();
	if let Err(e) = c.execute(
		"INSERT INTO Episodes (language, folder, title, status, speakers, createDate, updateDate)
		 VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?5)
		 ON CONFLICT(language, folder)
		 DO UPDATE SET status = ?3, speakers = ?4, updateDate = ?5",
		rusqlite::params![language, folder, status, speakers, ts],
	) {
		log_error_line("db", &format!("upsert Episodes 失敗 [{}]: {}", folder, e));
		return;
	}
	let episode_id: i64 = match c.query_row(
		"SELECT id FROM Episodes WHERE language = ?1 AND folder = ?2",
		rusqlite::params![language, folder],
		|r| r.get(0),
	) {
		Ok(id) => id,
		Err(_) => return,
	};

	// Lines 鏡射:有 word.txt 才灌;整批 delete + insert(trigger 同步 FTS)
	if let Ok(content) = std::fs::read_to_string(dir.join("word.txt")) {
		let _ = c.execute("DELETE FROM Lines WHERE episodeId = ?1", [episode_id]);
		let mut stmt = match c
			.prepare("INSERT INTO Lines (episodeId, lineNo, speaker, content) VALUES (?1, ?2, ?3, ?4)")
		{
			Ok(s) => s,
			Err(_) => return,
		};
		for (i, line) in content.lines().map(str::trim).filter(|l| !l.is_empty()).enumerate() {
			let (speaker, text) = match line.split_once("]:") {
				Some((s, t)) => (s.trim_start_matches('['), t.trim()),
				None => ("", line),
			};
			let _ = stmt.execute(rusqlite::params![episode_id, (i + 1) as i64, speaker, text]);
		}
	}
}

/// 下載完成:補上 sourceUrl 與原始標題(重建撈不回的欄位,只在此刻有)
pub fn record_download(language: &str, folder: &str, url: &str, title: &str) {
	refresh_episode(language, folder);
	let c = conn();
	let _ = c.execute(
		"UPDATE Episodes SET sourceUrl = ?3, title = ?4 WHERE language = ?1 AND folder = ?2",
		rusqlite::params![language, folder, url, title],
	);
}

pub fn delete_episode(language: &str, folder: &str) {
	let c = conn();
	// Lines 靠 FK CASCADE;primary 表(生字/對話/統計)刻意保留——使用者資產
	let _ = c.execute(
		"DELETE FROM Episodes WHERE language = ?1 AND folder = ?2",
		rusqlite::params![language, folder],
	);
}

/// 句級播放統計:每次 play_line +1(練習閉環的資料源)
pub fn bump_line_stat(language: &str, folder: &str, line_no: i32) {
	let c = conn();
	let _ = c.execute(
		"INSERT INTO LineStats (language, folder, lineNo, playCount, lastPlayed) VALUES (?1, ?2, ?3, 1, ?4)
		 ON CONFLICT(language, folder, lineNo) DO UPDATE SET playCount = playCount + 1, lastPlayed = ?4",
		rusqlite::params![language, folder, line_no, now()],
	);
}

// ── 生字本(primary)──

/// 標記/取消生字(同集同行同詞 = toggle);回傳 toggle 後是否為已標記
#[tauri::command]
pub fn toggle_vocab(
	folder: String,
	line_no: i32,
	term: String,
	reading: Option<String>,
	meaning: Option<String>,
	source: Option<String>,
) -> Result<bool, String> {
	crate::validate_folder(&folder)?;
	let term = term.trim().to_string();
	if term.is_empty() {
		return Err("空白詞".to_string());
	}
	let lang = crate::course_language();
	let c = conn();
	let deleted = c
		.execute(
			"DELETE FROM VocabItems WHERE language = ?1 AND folder = ?2 AND lineNo = ?3 AND term = ?4",
			rusqlite::params![lang, folder, line_no, term],
		)
		.map_err(|e| e.to_string())?;
	if deleted > 0 {
		return Ok(false);
	}
	c.execute(
		"INSERT INTO VocabItems (language, folder, lineNo, term, reading, meaning, source, addedDate)
		 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
		rusqlite::params![
			lang,
			folder,
			line_no,
			term,
			reading.unwrap_or_default(),
			meaning.unwrap_or_default(),
			source.unwrap_or_else(|| "user".to_string()),
			now()
		],
	)
	.map_err(|e| e.to_string())?;
	Ok(true)
}

/// 生字清單:給 folder 查單集,不給查目前課綱全部(新→舊)
#[tauri::command]
pub fn list_vocab(folder: Option<String>) -> Result<Vec<serde_json::Value>, String> {
	let lang = crate::course_language();
	let c = conn();
	let (sql, params): (&str, Vec<String>) = match &folder {
		Some(f) => (
			"SELECT id, folder, lineNo, term, reading, meaning, source, addedDate
			 FROM VocabItems WHERE language = ?1 AND folder = ?2 ORDER BY id DESC",
			vec![lang.clone(), f.clone()],
		),
		None => (
			"SELECT id, folder, lineNo, term, reading, meaning, source, addedDate
			 FROM VocabItems WHERE language = ?1 ORDER BY id DESC",
			vec![lang.clone()],
		),
	};
	let mut stmt = c.prepare(sql).map_err(|e| e.to_string())?;
	let rows = stmt
		.query_map(rusqlite::params_from_iter(params.iter()), |r| {
			Ok(serde_json::json!({
				"id": r.get::<_, i64>(0)?,
				"folder": r.get::<_, String>(1)?,
				"lineNo": r.get::<_, Option<i64>>(2)?,
				"term": r.get::<_, String>(3)?,
				"reading": r.get::<_, String>(4)?,
				"meaning": r.get::<_, String>(5)?,
				"source": r.get::<_, String>(6)?,
				"addedDate": r.get::<_, String>(7)?,
			}))
		})
		.map_err(|e| e.to_string())?;
	Ok(rows.flatten().collect())
}

/// 已標記生字的釋義快取查詢(AI 即查一次、終身快取的讀端)
pub fn get_vocab_meaning(language: &str, folder: &str, line_no: i32, term: &str) -> Option<String> {
	let c = conn();
	c.query_row(
		"SELECT meaning FROM VocabItems WHERE language = ?1 AND folder = ?2 AND lineNo = ?3 AND term = ?4",
		rusqlite::params![language, folder, line_no, term],
		|r| r.get::<_, String>(0),
	)
	.ok()
	.filter(|m| !m.is_empty())
}

/// AI 查得的詞性+釋義寫回生字(合併字串「【詞性】釋義」)
pub fn set_vocab_meaning(language: &str, folder: &str, line_no: i32, term: &str, meaning: &str) {
	let c = conn();
	let _ = c.execute(
		"UPDATE VocabItems SET meaning = ?5 WHERE language = ?1 AND folder = ?2 AND lineNo = ?3 AND term = ?4 AND meaning = ''",
		rusqlite::params![language, folder, line_no, term, meaning],
	);
}

// ── tutor 對話(primary)──

pub fn insert_message(language: &str, folder: &str, role: &str, content: &str, anchor_line: Option<i32>) {
	let c = conn();
	let _ = c.execute(
		"INSERT INTO Messages (language, folder, role, content, anchorLine, createDate) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
		rusqlite::params![language, folder, role, content, anchor_line, now()],
	);
}

/// 本集歷史對話(舊→新);bot context 與前端視窗共用
#[tauri::command]
pub fn list_messages(folder: String) -> Result<Vec<serde_json::Value>, String> {
	crate::validate_folder(&folder)?;
	let lang = crate::course_language();
	let c = conn();
	let mut stmt = c
		.prepare(
			"SELECT role, content, anchorLine, createDate FROM Messages
			 WHERE language = ?1 AND folder = ?2 ORDER BY id",
		)
		.map_err(|e| e.to_string())?;
	let rows = stmt
		.query_map(rusqlite::params![lang, folder], |r| {
			Ok(serde_json::json!({
				"role": r.get::<_, String>(0)?,
				"content": r.get::<_, String>(1)?,
				"anchorLine": r.get::<_, Option<i64>>(2)?,
				"createDate": r.get::<_, String>(3)?,
			}))
		})
		.map_err(|e| e.to_string())?;
	Ok(rows.flatten().collect())
}
