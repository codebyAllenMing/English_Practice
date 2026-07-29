# English Practice

用 YouTube Podcast 練習英文聽力的 macOS 桌面應用(Apple Silicon)。

貼上連結 → 本地轉譯(含說話者辨識)→ AI 校正 → 逐句 TTS 跟讀練習。
**推論全部在本機原生執行**:whisper.cpp(Metal)+ sherpa-onnx,不需雲端、不需 Python 環境、模型下載後完全離線。

> 架構與流程圖:[docs/architecture.html](docs/architecture.html)

## 特色

- **原生轉譯管線**——whisper.cpp(Metal GPU)逐詞時間戳 + pyannote 說話者分離,句子優先斷行、行內多數決歸戶
- **AI 校正(雙模式)**——Anthropic API(structured outputs 強制 JSON)或本機 Claude CLI;修正聽錯的字、標點,辨識講者真名與性別;只修錯不改寫,保住聽力素材保真
- **逐句 TTS 練習**——kokoro 語音合成,依講者性別自動配音(男/女聲池),可手動指定 12 種美音
- **首次啟動自動初始化**——模型下載器(953MB,SHA256 驗證),之後全離線

## 素材來源

任何 YouTube 英文 Podcast / 對話影片皆可,實測用的參考頻道:

- [English Podcast](https://www.youtube.com/@EnglishPodcast1314)
- [Speak English With Class](https://www.youtube.com/@SpeakEnglishWithClass)
- [The Learning Lab](https://www.youtube.com/@thelearninglab-h1k)

> **僅支援英文**:轉譯管線固定以英文辨識(`language = en`),其他語言的影片不支援。

## 效能

| 指標 | 實測(M2) |
| --- | --- |
| 轉譯速度 | 16.6 分鐘音檔約 **3.5 分鐘**(whisper Metal,約 9.5x 即時速) |
| TTS | 模型載入 **0.7s**,每句合成約 2s |
| 體積 | app 本體 ~25MB(.dmg 22MB);模型 953MB 下載一次、永久離線 |
| 帳號需求 | 零——不需 HuggingFace token、不需任何雲端服務(AI 校正除外) |

## 安裝(使用者)

1. 下載 `.dmg`,把 app 拖進「應用程式」
2. 未簽名版本首次開啟:**右鍵 → 打開**
3. 安裝外部工具:`brew install yt-dlp ffmpeg`
4. 首次啟動依畫面指示下載模型(約 953MB,一次性)

資料存放於 `~/Library/Application Support/com.allenming.english-practice/`;解除安裝 = 刪 app + 刪此資料夾。

## 開發

需求:macOS(Apple Silicon)、[Rust](https://rustup.rs)、Node.js 20+、`brew install cmake yt-dlp ffmpeg`

```bash
cd app
npm install
npm run tauri dev      # dev 模式資料在專案根目錄
npm run tauri build    # 產出 .app / .dmg
```

headless 測試工具(不開 GUI 直接跑管線):

```bash
cd app/src-tauri
cargo run --release --bin native_test <資料夾>                      # 轉譯
cargo run --release --bin native_test tts <資料夾> <行號> <out.wav>  # TTS
```

## AI 校正模式

| 模式 | 適用 | 需求 |
| --- | --- | --- |
| **Anthropic API** | 一般使用者 | API key(存 macOS Keychain,不落地明文) |
| **本機 Claude CLI** | 已安裝並登入 [Claude Code](https://claude.com/claude-code) 的開發者 | 吃訂閱額度,免 key |

## 練習頁鍵盤操作

- **↓ / 空白鍵** — 下一句 **↑** — 上一句 **←** — 重複

## 安全設計

API key 進 Keychain、模型 SHA256 驗證、路徑跳脫防護、嚴格 CSP、AI 僅回建議清單(套用全在本地驗證)。詳見[架構文件](docs/architecture.html)。

## Roadmap

- [ ] 自動更新(Tauri updater + GitHub Releases)
- [ ] 生字本:練習中標記單字 → n8n webhook → Google Sheets
- [ ] 簽名與公證(Apple Developer)
- [ ] CI 自動建置發佈

## 技術棧

Tauri 2 · Rust(tokio / whisper-rs / sherpa-onnx / reqwest)· React 19 · Vite · Tailwind v4 · Anthropic API(claude-haiku-4-5, structured outputs)
