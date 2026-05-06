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
struct ClaudeProcess(Mutex<Option<PersistentChild>>);

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
    let pc = spawn_persistent("practice.py").await?;
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

    Err("practice.py 意外結束".to_string())
}

#[derive(serde::Serialize)]
struct TitleInfo {
    title: String,
    folder: String,
}

#[tauri::command]
async fn fetch_title(url: String) -> Result<TitleInfo, String> {
    let output = Command::new(python_path())
        .arg("download.py")
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
    cmd.arg("download.py").arg(&url);
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
        .arg("transcribe.py")
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
async fn start_claude(app: AppHandle) -> Result<(), String> {
    let state = app.state::<ClaudeProcess>();
    let mut guard = state.0.lock().await;
    if guard.is_some() {
        return Ok(());
    }
    let pc = spawn_persistent("fix_speakers.py --daemon").await?;
    *guard = Some(pc);
    Ok(())
}

#[tauri::command]
async fn stop_claude(app: AppHandle) -> Result<(), String> {
    let state = app.state::<ClaudeProcess>();
    let mut guard = state.0.lock().await;
    if let Some(mut pc) = guard.take() {
        let _ = pc.stdin.write_all(b"QUIT\n").await;
        let _ = pc.child.kill().await;
    }
    Ok(())
}

#[tauri::command]
async fn fix_speakers(app: AppHandle, word_path: String) -> Result<String, String> {
    let state = app.state::<ClaudeProcess>();
    let mut guard = state.0.lock().await;
    let pc = guard.as_mut().ok_or("Claude session 未啟動")?;

    pc.stdin.write_all(format!("{}\n", word_path).as_bytes()).await.map_err(|e| e.to_string())?;
    pc.stdin.flush().await.map_err(|e| e.to_string())?;

    while let Some(line) = pc.reader.next_line().await.map_err(|e| e.to_string())? {
        if let Some(msg) = line.strip_prefix("DONE:") {
            return Ok(msg.to_string());
        } else if let Some(err) = line.strip_prefix("ERROR:") {
            return Err(err.to_string());
        }
    }

    Err("fix_speakers.py 意外結束".to_string())
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
        .manage(ClaudeProcess(Mutex::new(None)))
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
                transcribe_audio, get_config, save_config, get_lines,
                start_practice, stop_practice, play_line,
                start_claude, stop_claude, fix_speakers
            ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
