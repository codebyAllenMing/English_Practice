# English Practice

用 YouTube Podcast 練習英文聽力與口說的桌面應用程式。

## 功能

- **下載** — 貼上 YouTube 連結，自動下載音檔並整理資料夾
- **轉譯** — 使用 whisperx 將音檔轉成帶說話者標記的文字，並透過 Claude CLI 自動校正說話者名字
- **練習** — 使用 Kokoro TTS 逐句朗讀，支援鍵盤控制（上一句、下一句、重複）

## 技術架構

```
Tauri (React + Rust) → Python 後端 → Kokoro TTS / whisperx / yt-dlp
```

- **前端**：React + Tailwind CSS
- **桌面殼**：Tauri（Rust）
- **TTS**：Kokoro（離線）
- **語音轉文字**：whisperx + pyannote（離線）
- **下載**：yt-dlp
- **說話者校正**：Claude CLI

## 環境需求

- macOS（Apple Silicon）
- Python 3.11+
- Node.js 20+
- Rust
- ffmpeg（`brew install ffmpeg`）
- yt-dlp（`brew install yt-dlp`）
- Claude CLI（已登入）

## 安裝

```bash
# Python 環境
python3 -m venv venv
source venv/bin/activate
pip install kokoro sounddevice soundfile numpy faster-whisper whisperx

# Node 環境
cd app
npm install
```

## HuggingFace 設定（轉譯用）

1. 申請模型存取權限：
   - https://huggingface.co/pyannote/segmentation-3.0
   - https://huggingface.co/pyannote/speaker-diarization-community-1

2. 產生 Access Token（read 權限）：
   - https://huggingface.co/settings/tokens

3. 在 app 設定頁面（齒輪 icon）輸入 token

## 啟動

### 桌面應用程式（完整功能）

```bash
source venv/bin/activate
nvm use 20 && npm --prefix app run tauri dev
```

### 也可以使用 Automator 建立桌面 app，點兩下直接啟動（見 start.command）

## 使用流程

### 1. 下載

在「下載」頁面貼上 YouTube 連結，按下載。自動建立資料夾並轉成 MP3。

### 2. 轉譯

在「轉譯」頁面選擇未轉譯的 podcast，按開始轉譯。完成後自動：
- 產生 `word.txt`（帶說話者標記）
- 透過 Claude CLI 校正說話者名字
- 刪除 MP3 音檔

### 3. 練習

在「練習」頁面選擇 podcast，使用 Kokoro TTS 逐句播放。

操作方式：
- **↓ / 空白鍵** — 下一句
- **↑** — 上一句
- **←** — 重複當前句
- **A+ / A-** — 調整字體大小
- 點擊下方句子列表可跳到任意句

## 單獨測試（Terminal）

每個功能都可以在 terminal 單獨跑，不需要啟動 Tauri app。

### 下載

```bash
source venv/bin/activate
python3 download.py "https://www.youtube.com/watch?v=XXXX"
```

### 轉譯

```bash
source venv/bin/activate
python3 transcribe.py <資料夾名稱>
# 例如：python3 transcribe.py Why_People_Sound_Rude_speaking_in_English
```

### 說話者校正

```bash
source venv/bin/activate
python3 fix_speakers.py podcasts/<資料夾名稱>/word.txt
```

### TTS 語音測試（常駐模式）

```bash
source venv/bin/activate
python3 practice.py
# 輸入 JSON 指令：{"folder": "資料夾名稱", "index": 0}
# 輸入 QUIT 結束
```

### 查看錯誤日誌

```bash
cat logs/error.log
```

## 專案結構

```
English_Practice/
  app/                    ← Tauri + React 前端
    src/
      Pages/              ← 頁面（Download, Transcribe, Practice）
      Components/         ← 共用元件（Settings）
      Hooks/              ← 自訂 Hooks（useLoading, useTranscribe）
    src-tauri/            ← Rust 後端
  download.py             ← YouTube 下載邏輯
  transcribe.py           ← 語音轉文字邏輯
  fix_speakers.py         ← Claude CLI 說話者校正
  practice.py             ← Kokoro TTS 語音產生（常駐）
  env_setup.py            ← 環境 PATH 設定
  logger.py               ← 錯誤日誌
  podcasts/               ← podcast 資料（word.txt）
  VOICES.md               ← Kokoro 可用語音列表
```
