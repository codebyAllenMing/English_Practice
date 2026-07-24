use std::fs;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

fn project_dir() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap();
    while dir.file_name().map(|f| f != "English_Practice").unwrap_or(false) {
        dir = dir.parent().unwrap().to_path_buf();
    }
    dir
}

fn python_path() -> PathBuf {
    project_dir().join("venv").join("bin").join("python3")
}

struct PracticeProcess(Mutex<Option<PersistentChild>>);

struct PersistentChild {
    child: Child,
    stdin: tokio::process::ChildStdin,
    reader: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
}

async fn spawn_persistent(script: &str) -> Result<PersistentChild, String> {
    let args: Vec<&str> = script.split_whitespace().collect();
    let mut child = Command::new(python_path())
        .args(&args)
        .current_dir(project_dir())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("無法啟動 {}: {}", script, e))?;

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout).lines();

    // 等待 READY
    while let Some(line) = reader.next_line().await.map_err(|e| e.to_string())? {
        if line == "READY" {
            break;
        }
    }

    Ok(PersistentChild { child, stdin, reader })
}

#[tauri::command]
async fn start_practice(app: AppHandle) -> Result<(), String> {
    let state = app.state::<PracticeProcess>();
    let mut guard = state.0.lock().await;
    if guard.is_some() {
        return Ok(());
    }
    let pc = spawn_persistent("-m scripts.practice").await?;
    *guard = Some(pc);
    Ok(())
}

#[tauri::command]
async fn stop_practice(app: AppHandle) -> Result<(), String> {
    let state = app.state::<PracticeProcess>();
    let mut guard = state.0.lock().await;
    if let Some(mut pc) = guard.take() {
        let _ = pc.stdin.write_all(b"QUIT\n").await;
        let _ = pc.child.kill().await;
    }
    Ok(())
}

#[tauri::command]
async fn play_line(app: AppHandle, folder: String, index: i32) -> Result<serde_json::Value, String> {
    let state = app.state::<PracticeProcess>();
    let mut guard = state.0.lock().await;
    let pc = guard.as_mut().ok_or("練習模式未啟動")?;

    let cmd = serde_json::json!({"folder": folder, "index": index});
    pc.stdin.write_all(format!("{}\n", cmd).as_bytes()).await.map_err(|e| e.to_string())?;
    pc.stdin.flush().await.map_err(|e| e.to_string())?;

    while let Some(line) = pc.reader.next_line().await.map_err(|e| e.to_string())? {
        if let Some(json_str) = line.strip_prefix("RESULT:") {
            let val: serde_json::Value = serde_json::from_str(json_str).map_err(|e| e.to_string())?;
            return Ok(val);
        } else if let Some(err) = line.strip_prefix("ERROR:") {
            return Err(err.to_string());
        }
    }

    Err("scripts.practice 意外結束".to_string())
}

#[derive(serde::Serialize)]
struct TitleInfo {
    title: String,
    folder: String,
}

#[tauri::command]
async fn fetch_title(url: String) -> Result<TitleInfo, String> {
    let output = Command::new(python_path())
        .arg("-m")
        .arg("scripts.download")
        .arg("--fetch-title")
        .arg(&url)
        .current_dir(project_dir())
        .output()
        .await
        .map_err(|e| format!("無法執行: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let mut title = String::new();
    let mut folder = String::new();

    for line in stdout.lines() {
        if let Some(t) = line.strip_prefix("TITLE:") {
            title = t.to_string();
        } else if let Some(f) = line.strip_prefix("FOLDER:") {
            folder = f.to_string();
        } else if let Some(err) = line.strip_prefix("ERROR:") {
            return Err(err.to_string());
        }
    }

    if title.is_empty() {
        return Err(format!("無法取得標題\n{}", stderr));
    }

    Ok(TitleInfo { title, folder })
}

#[tauri::command]
async fn download_audio(app: AppHandle, url: String, folder: Option<String>) -> Result<String, String> {
    let mut cmd = Command::new(python_path());
    cmd.arg("-m").arg("scripts.download").arg(&url);
    if let Some(f) = &folder {
        cmd.arg(f);
    }

    let mut child = cmd
        .current_dir(project_dir())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("無法執行: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout).lines();
    let mut result = String::new();

    while let Some(line) = reader.next_line().await.map_err(|e| e.to_string())? {
        if let Some(pct) = line.strip_prefix("PROGRESS:") {
            let _ = app.emit("download-progress", pct.to_string());
        } else if let Some(title) = line.strip_prefix("TITLE:") {
            let _ = app.emit("download-title", title.to_string());
        } else if let Some(path) = line.strip_prefix("DONE:") {
            result = path.to_string();
        } else if let Some(err) = line.strip_prefix("ERROR:") {
            return Err(err.to_string());
        }
    }

    let status = child.wait().await.map_err(|e| e.to_string())?;
    if status.success() {
        Ok(result)
    } else {
        Err("下載失敗".to_string())
    }
}

#[derive(serde::Serialize)]
struct PodcastInfo {
    name: String,
    transcribed: bool,
    corrected: bool,
}

#[tauri::command]
fn list_podcasts() -> Result<Vec<PodcastInfo>, String> {
    let podcasts_dir = project_dir().join("podcasts");
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
    let podcasts_dir = project_dir().join("podcasts");
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

#[tauri::command]
async fn transcribe_audio(app: AppHandle, folder: String) -> Result<String, String> {
    let mut child = Command::new(python_path())
        .arg("-m")
        .arg("scripts.transcribe")
        .arg(&folder)
        .current_dir(project_dir())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("無法執行: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout).lines();
    let mut result = String::new();

    while let Some(line) = reader.next_line().await.map_err(|e| e.to_string())? {
        if let Some(msg) = line.strip_prefix("PROGRESS:") {
            let _ = app.emit("transcribe-progress", msg.to_string());
        } else if let Some(path) = line.strip_prefix("DONE:") {
            result = path.to_string();
        } else if let Some(err) = line.strip_prefix("ERROR:") {
            return Err(err.to_string());
        }
    }

    let status = child.wait().await.map_err(|e| e.to_string())?;
    if status.success() {
        Ok(result)
    } else {
        Err("轉譯失敗".to_string())
    }
}

#[derive(serde::Deserialize)]
struct SpeakerRename {
    from: String,
    to: String,
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

/// AI 校正:speaker 對照 + 逐行 ASR 錯字/標點修正,一次 API 呼叫完成。
/// 輸入永遠是 word.raw.txt(不修改),輸出覆寫 word.txt 並存 correction.json。
#[tauri::command]
async fn correct_transcript(folder: String) -> Result<serde_json::Value, String> {
    let folder_path = project_dir().join("podcasts").join(&folder);
    let raw_path = folder_path.join("word.raw.txt");
    let word_path = folder_path.join("word.txt");

    // 舊資料相容:沒有 word.raw.txt 就從現有 word.txt 建一份
    if !raw_path.exists() {
        if !word_path.exists() {
            return Err("找不到逐字稿,請先轉譯".to_string());
        }
        fs::copy(&word_path, &raw_path).map_err(|e| format!("建立 word.raw.txt 失敗: {}", e))?;
    }

    let api_key = fs::read_to_string(project_dir().join("config.json"))
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .and_then(|v| v["anthropic_api_key"].as_str().map(|s| s.to_string()))
        .filter(|k| !k.is_empty())
        .ok_or("請先在設定填入 Anthropic API Key")?;

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
        1. speakers: determine each speaker's real name from the dialogue (introductions like \"My name is ...\", \
        or speakers addressing each other). Output one entry per label you can confidently rename, mapping the \
        original label (e.g. SPEAKER_00) to the real name. Omit labels you cannot determine. \
        Names must not contain '[', ']' or ':'.\n\
        2. fixes: find lines containing obvious speech-to-text errors (misheard words, broken punctuation) and \
        output the corrected full line. 'line' is the line number; 'text' is the complete replacement line \
        WITHOUT the number prefix but INCLUDING the original \"[SPEAKER_XX]: \" prefix unchanged.\n\n\
        Strict rules for fixes:\n\
        - Only fix obviously mis-transcribed words and punctuation.\n\
        - Do NOT rephrase, do NOT fix grammar the speaker actually said, do NOT remove filler words, \
        do NOT merge or split lines.\n\
        - Only include lines that actually need a change.\n\n\
        Transcript:\n{}",
        numbered
    );

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
                                    "to": { "type": "string" }
                                },
                                "required": ["from", "to"],
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
        .header("x-api-key", &api_key)
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

    let text = resp_json["content"]
        .as_array()
        .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
        .and_then(|b| b["text"].as_str())
        .ok_or("API 回應缺少文字內容")?;

    let correction: Correction =
        serde_json::from_str(text).map_err(|e| format!("解析校正結果失敗: {}", e))?;

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
    fs::write(
        folder_path.join("correction.json"),
        serde_json::to_string_pretty(&resp_json).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("寫入 correction.json 失敗: {}", e))?;

    Ok(serde_json::json!({
        "speakers": correction.speakers.len(),
        "fixes": correction.fixes.len(),
        "skipped": skipped,
    }))
}

#[tauri::command]
fn list_transcribed() -> Result<Vec<String>, String> {
    let podcasts_dir = project_dir().join("podcasts");
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
    let word_path = project_dir().join("podcasts").join(&folder).join("word.txt");
    let content = fs::read_to_string(&word_path).map_err(|e| e.to_string())?;
    Ok(content.lines().filter(|l| !l.trim().is_empty()).map(|l| l.to_string()).collect())
}

#[tauri::command]
fn get_config() -> Result<serde_json::Value, String> {
    let config_path = project_dir().join("config.json");
    if !config_path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_config(config: serde_json::Value) -> Result<(), String> {
    let config_path = project_dir().join("config.json");
    let content = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&config_path, content).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(PracticeProcess(Mutex::new(None)))
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
                fetch_title, download_audio, list_podcasts, list_untranscribed, list_transcribed,
                transcribe_audio, correct_transcript, get_config, save_config, get_lines,
                start_practice, stop_practice, play_line
            ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
