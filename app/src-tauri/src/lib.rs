use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

pub mod native_download;
pub mod native_models;
pub mod native_transcribe;
pub mod native_tts;

pub fn project_dir() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap();
    while dir.file_name().map(|f| f != "English_Practice").unwrap_or(false) {
        dir = dir.parent().unwrap().to_path_buf();
    }
    dir
}

/// 資料根目錄(podcasts/models/config.json/logs 的家)。
/// 優先序:init_data_dir 顯式指定(native_test 用)> debug 建置 = 專案根(開發資料不搬家)
/// > release = ~/Library/Application Support/<bundle id>(打包後的正式位置)
static DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn init_data_dir(path: PathBuf) {
    let _ = DATA_DIR.set(path);
}

pub(crate) fn data_dir() -> &'static PathBuf {
    DATA_DIR.get_or_init(|| {
        let dir = if cfg!(debug_assertions) {
            project_dir()
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join("Library/Application Support/com.allenming.english-practice")
        };
        let _ = fs::create_dir_all(dir.join("podcasts"));
        let _ = fs::create_dir_all(dir.join("models"));
        dir
    })
}

/// 外部 CLI 尋找:PATH 之外補上 Homebrew(arm64/Intel)路徑——
/// 從 Finder 啟動的 .app 拿不到 shell 的 PATH
pub(crate) fn find_tool(name: &str) -> Option<PathBuf> {
    for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let p = PathBuf::from(dir).join(name);
        if p.exists() {
            return Some(p);
        }
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(name))
            .find(|p| p.exists())
    })
}

#[derive(serde::Serialize)]
struct PodcastInfo {
    name: String,
    transcribed: bool,
    corrected: bool,
}

#[tauri::command]
fn list_podcasts() -> Result<Vec<PodcastInfo>, String> {
    let podcasts_dir = data_dir().join("podcasts");
    if !podcasts_dir.exists() {
        return Ok(vec![]);
    }
    let mut list: Vec<PodcastInfo> = fs::read_dir(&podcasts_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            let dir = entry.path();
            Some(PodcastInfo {
                name: entry.file_name().to_string_lossy().to_string(),
                transcribed: dir.join("word.txt").exists(),
                corrected: dir.join("correction.json").exists(),
            })
        })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

#[tauri::command]
fn list_untranscribed() -> Result<Vec<String>, String> {
    let podcasts_dir = data_dir().join("podcasts");
    if !podcasts_dir.exists() {
        return Ok(vec![]);
    }
    let mut list: Vec<String> = fs::read_dir(&podcasts_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            let dir = entry.path();
            let has_audio = dir.join("podcast.mp3").exists();
            let has_text = dir.join("word.txt").exists();
            if has_audio && !has_text {
                Some(entry.file_name().to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();
    list.sort();
    Ok(list)
}

/// 與 core/logger.py 同格式寫入 logs/:[YYYY-mm-dd HH:MM:SS] [source] message
fn log_line(file: &str, source: &str, message: &str) {
    let log_dir = data_dir().join("logs");
    let _ = fs::create_dir_all(&log_dir);
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let line = format!("[{}] [{}] {}\n", timestamp, source, message);
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join(file))
    {
        use std::io::Write;
        let _ = f.write_all(line.as_bytes());
    }
}

fn log_info_line(source: &str, message: &str) {
    log_line("info.log", source, message);
}

fn log_error_line(source: &str, message: &str) {
    log_line("error.log", source, message);
}

/// 進行中任務註冊表:防止同一資料夾的長任務(校正/轉譯)被重複觸發
fn running_tasks() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static RUNNING: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    RUNNING.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// 註冊成功回傳 guard(drop 時自動解除,含錯誤提早 return 的路徑);已在跑則回 Err
fn try_begin_task(kind: &str, folder: &str) -> Result<TaskGuard, String> {
    let key = format!("{}:{}", kind, folder);
    let mut running = running_tasks().lock().unwrap();
    if !running.insert(key.clone()) {
        return Err(format!("「{}」{}進行中,請稍候", folder, kind));
    }
    Ok(TaskGuard(key))
}

struct TaskGuard(String);

impl Drop for TaskGuard {
    fn drop(&mut self) {
        running_tasks().lock().unwrap().remove(&self.0);
    }
}

#[derive(serde::Deserialize)]
struct SpeakerRename {
    from: String,
    to: String,
    // 舊 correction.json 沒有此欄位;TTS 端讀 correction.json 挑男/女聲池用
    #[serde(default)]
    #[allow(dead_code)]
    gender: Option<String>,
}

#[derive(serde::Deserialize)]
struct LineFix {
    line: i64,
    text: String,
}

#[derive(serde::Deserialize)]
struct Correction {
    speakers: Vec<SpeakerRename>,
    fixes: Vec<LineFix>,
}

/// AI 校正:speaker 對照 + 逐行 ASR 錯字/標點修正,一次呼叫完成。
/// 輸入永遠是 word.raw.txt(不修改),輸出覆寫 word.txt 並存 correction.json。
#[tauri::command]
async fn correct_transcript(folder: String) -> Result<serde_json::Value, String> {
    let _guard = try_begin_task("校正", &folder)?;
    let start = std::time::Instant::now();
    log_info_line("correct", &format!("開始校正: {}", folder));
    match correct_transcript_inner(&folder).await {
        Ok(res) => {
            log_info_line(
                "correct",
                &format!(
                    "[{}] 校正完成 (耗時 {:.1}s, mode {}, 說話者 {} 位, 修正 {} 行, 略過 {} 行)",
                    folder,
                    start.elapsed().as_secs_f32(),
                    res["mode"].as_str().unwrap_or("?"),
                    res["speakers"],
                    res["fixes"],
                    res["skipped"],
                ),
            );
            Ok(res)
        }
        Err(e) => {
            log_error_line(
                "correct",
                &format!("[{}] 校正失敗 (耗時 {:.1}s): {}", folder, start.elapsed().as_secs_f32(), e),
            );
            Err(e)
        }
    }
}

async fn correct_transcript_inner(folder: &str) -> Result<serde_json::Value, String> {
    let folder_path = data_dir().join("podcasts").join(folder);
    let raw_path = folder_path.join("word.raw.txt");
    let word_path = folder_path.join("word.txt");

    // 舊資料相容:沒有 word.raw.txt 就從現有 word.txt 建一份
    if !raw_path.exists() {
        if !word_path.exists() {
            return Err("找不到逐字稿,請先轉譯".to_string());
        }
        fs::copy(&word_path, &raw_path).map_err(|e| format!("建立 word.raw.txt 失敗: {}", e))?;
    }

    let config: serde_json::Value = fs::read_to_string(data_dir().join("config.json"))
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    // 校正模式:"api"(Anthropic API,key 計費)或 "cli"(本機 Claude CLI,吃訂閱額度)
    let mode = config["correction_mode"].as_str().unwrap_or("api").to_string();

    let raw_text = fs::read_to_string(&raw_path).map_err(|e| format!("讀取 word.raw.txt 失敗: {}", e))?;
    let mut lines: Vec<String> = raw_text.lines().map(|l| l.to_string()).collect();

    let numbered = lines
        .iter()
        .enumerate()
        .map(|(i, l)| format!("{}|{}", i + 1, l))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "Below is a podcast transcript. Each line is prefixed with its 1-based line number followed by '|'. \
        Lines have the format \"[SPEAKER_XX]: text\".\n\n\
        Do two things:\n\
        1. speakers: output one entry for EVERY distinct speaker label appearing in the transcript — \
        including labels that are already real names. 'from' is the label exactly as it appears \
        (e.g. SPEAKER_00, or James if already named). 'to' is the real name determined from the dialogue \
        (introductions like \"My name is ...\", speakers addressing each other); if the label is already \
        the real name, or the name cannot be determined, set 'to' identical to 'from'. \
        Speaker diarization sometimes over-splits: a small leftover label (few lines) whose lines clearly \
        continue an existing speaker's dialogue (same conversational flow, role-play, brief interjections) \
        should be mapped to that speaker's name instead of kept separate. \
        Names must not contain '[', ']' or ':'. \
        For each entry also set gender: \"m\" (male), \"f\" (female), or \"u\" (unknown) — judge from the name, \
        pronouns, and how speakers address each other; used to pick a matching TTS voice.\n\
        2. fixes: find lines containing obvious speech-to-text errors (misheard words, broken punctuation) and \
        output the corrected full line. 'line' is the line number; 'text' is the complete replacement line \
        WITHOUT the number prefix but INCLUDING the original \"[SPEAKER_XX]: \" prefix unchanged.\n\n\
        Strict rules for fixes:\n\
        - Only fix obviously mis-transcribed words and punctuation.\n\
        - Do NOT rephrase, do NOT fix grammar the speaker actually said, do NOT remove filler words, \
        do NOT merge or split lines.\n\
        - Only include lines that actually need a change.\n\n\
        Respond with ONLY a JSON object of the shape \
        {{\"speakers\": [{{\"from\": \"SPEAKER_00\", \"to\": \"Name\", \"gender\": \"m\"}}], \
        \"fixes\": [{{\"line\": 1, \"text\": \"[SPEAKER_00]: corrected line\"}}]}} \
        — no markdown fences, no explanations.\n\n\
        Transcript:\n{}",
        numbered
    );

    let result_text = if mode == "cli" {
        correct_via_cli(&prompt).await?
    } else {
        let api_key = config["anthropic_api_key"]
            .as_str()
            .filter(|k| !k.is_empty())
            .ok_or("請先在設定填入 Anthropic API Key(或切換為本機 Claude CLI 模式)")?;
        correct_via_api(api_key, &prompt).await?
    };

    let parsed: serde_json::Value = serde_json::from_str(extract_json(&result_text))
        .map_err(|e| format!("解析校正結果失敗: {}", e))?;
    let correction: Correction = serde_json::from_value(parsed.clone())
        .map_err(|e| format!("校正結果格式不符: {}", e))?;

    // 套逐行修正(行號 1-based,越界丟棄)
    let mut skipped = 0;
    for fix in &correction.fixes {
        if fix.line >= 1 && (fix.line as usize) <= lines.len() {
            lines[(fix.line - 1) as usize] = fix.text.clone();
        } else {
            skipped += 1;
        }
    }

    // 套 speaker 名字(只替換行首的 [LABEL]: 前綴)
    for sp in &correction.speakers {
        if sp.from == sp.to {
            continue;
        }
        let from_prefix = format!("[{}]:", sp.from);
        let to_prefix = format!("[{}]:", sp.to);
        for line in lines.iter_mut() {
            if line.starts_with(&from_prefix) {
                *line = line.replacen(&from_prefix, &to_prefix, 1);
            }
        }
    }

    fs::write(&word_path, lines.join("\n")).map_err(|e| format!("寫入 word.txt 失敗: {}", e))?;
    let record = serde_json::json!({ "mode": mode, "result": parsed });
    fs::write(
        folder_path.join("correction.json"),
        serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("寫入 correction.json 失敗: {}", e))?;

    Ok(serde_json::json!({
        "speakers": correction.speakers.len(),
        "fixes": correction.fixes.len(),
        "skipped": skipped,
        "mode": mode,
    }))
}

/// Anthropic API 路徑:structured outputs 強制 JSON 回傳
async fn correct_via_api(api_key: &str, prompt: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": "claude-haiku-4-5",
        "max_tokens": 8192,
        "output_config": {
            "format": {
                "type": "json_schema",
                "schema": {
                    "type": "object",
                    "properties": {
                        "speakers": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "from": { "type": "string" },
                                    "to": { "type": "string" },
                                    "gender": { "type": "string", "enum": ["m", "f", "u"] }
                                },
                                "required": ["from", "to", "gender"],
                                "additionalProperties": false
                            }
                        },
                        "fixes": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "line": { "type": "integer" },
                                    "text": { "type": "string" }
                                },
                                "required": ["line", "text"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "required": ["speakers", "fixes"],
                    "additionalProperties": false
                }
            }
        },
        "messages": [{ "role": "user", "content": prompt }]
    });

    let resp = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("API 連線失敗: {}", e))?;

    let status = resp.status();
    let resp_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("讀取 API 回應失敗: {}", e))?;

    if !status.is_success() {
        let msg = resp_json["error"]["message"].as_str().unwrap_or("未知錯誤");
        return Err(format!("API 錯誤 ({}): {}", status.as_u16(), msg));
    }
    if resp_json["stop_reason"] == "max_tokens" {
        return Err("校正結果超過輸出上限,結果不完整,未套用".to_string());
    }

    resp_json["content"]
        .as_array()
        .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
        .and_then(|b| b["text"].as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "API 回應缺少文字內容".to_string())
}

/// 本機 Claude CLI 路徑:單次 print 呼叫(無 agentic loop、不給工具),吃使用者登入的訂閱額度
async fn correct_via_cli(prompt: &str) -> Result<String, String> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        Command::new("claude")
            .args(["-p", prompt, "--model", "haiku", "--no-session-persistence"])
            .current_dir(data_dir())
            .output(),
    )
    .await
    .map_err(|_| "Claude CLI 逾時(300 秒)".to_string())?
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "找不到 claude CLI,請確認已安裝並登入,或到設定切換為 Anthropic API 模式".to_string()
        } else {
            format!("無法執行 claude CLI: {}", e)
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Claude CLI 失敗: {}", stderr.chars().take(300).collect::<String>()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 模型輸出可能帶 markdown fence 或前後綴文字,取第一個 '{' 到最後一個 '}'
fn extract_json(text: &str) -> &str {
    match (text.find('{'), text.rfind('}')) {
        (Some(s), Some(e)) if e > s => &text[s..=e],
        _ => text,
    }
}

/// 刪除整個 podcast 資料夾(音檔/逐字稿/校正紀錄一併移除)
#[tauri::command]
fn delete_podcast(folder: String) -> Result<(), String> {
    // 防路徑跳脫:只接受單層資料夾名
    if folder.is_empty() || folder.contains('/') || folder.contains('\\') || folder.contains("..") {
        return Err("無效的資料夾名稱".to_string());
    }
    let dir = data_dir().join("podcasts").join(&folder);
    if !dir.is_dir() {
        return Err("找不到資料夾".to_string());
    }
    fs::remove_dir_all(&dir).map_err(|e| format!("刪除失敗: {}", e))
}

#[tauri::command]
fn list_transcribed() -> Result<Vec<String>, String> {
    let podcasts_dir = data_dir().join("podcasts");
    if !podcasts_dir.exists() {
        return Ok(vec![]);
    }
    let mut list: Vec<String> = fs::read_dir(&podcasts_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            if entry.path().join("word.txt").exists() {
                Some(entry.file_name().to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();
    list.sort();
    Ok(list)
}

#[tauri::command]
fn get_lines(folder: String) -> Result<Vec<String>, String> {
    let word_path = data_dir().join("podcasts").join(&folder).join("word.txt");
    let content = fs::read_to_string(&word_path).map_err(|e| e.to_string())?;
    Ok(content.lines().filter(|l| !l.trim().is_empty()).map(|l| l.to_string()).collect())
}

#[tauri::command]
fn get_config() -> Result<serde_json::Value, String> {
    let config_path = data_dir().join("config.json");
    if !config_path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_config(config: serde_json::Value) -> Result<(), String> {
    let config_path = data_dir().join("config.json");
    let content = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&config_path, content).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(native_tts::TtsState(tokio::sync::Mutex::new(None)))
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
                native_download::fetch_title, native_download::download_audio,
                list_podcasts, list_untranscribed, list_transcribed,
                native_transcribe::transcribe_audio, correct_transcript, delete_podcast, get_config, save_config, get_lines,
                native_tts::start_practice, native_tts::stop_practice, native_tts::play_line,
                native_tts::get_voices, native_tts::save_voices,
                native_models::models_status, native_models::download_models, native_download::tools_status
            ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
