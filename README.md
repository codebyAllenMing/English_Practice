# English Practice

用 YouTube Podcast 練習英文聽力的桌面應用程式。

## 功能

- **下載** YouTube 音檔
- **轉譯** 成文字（含說話者辨識）
- **練習** 逐句 TTS 播放

## 環境需求

- macOS（Apple Silicon）
- Python 3.11+、Node.js 20+、Rust
- `brew install ffmpeg yt-dlp`
- Claude CLI（已登入）

## 安裝

```bash
python3 -m venv venv
source venv/bin/activate
pip install -r requirements-direct.txt

cd app && npm install
```

## 設定

到 [HuggingFace](https://huggingface.co/settings/tokens) 申請 read token，啟動 app 後在齒輪設定輸入。

需要先申請存取權限：
- [pyannote/segmentation-3.0](https://huggingface.co/pyannote/segmentation-3.0)
- [pyannote/speaker-diarization-community-1](https://huggingface.co/pyannote/speaker-diarization-community-1)

## 啟動

### 方式 1：Terminal

```bash
source venv/bin/activate
nvm use 20 && npm --prefix app run tauri dev
```

### 方式 2：桌面捷徑

用 macOS 的 Automator 建立 Application，內容貼上：

```bash
cd /Users/allen/Documents/Developer/English_Practice
source venv/bin/activate
export PATH="$HOME/.nvm/versions/node/v20.20.0/bin:$PATH"
npm --prefix app run tauri dev
```

存到桌面後，點兩下圖示即可開啟。

## 鍵盤操作（練習頁）

- **↓ / 空白鍵** — 下一句
- **↑** — 上一句
- **←** — 重複
