# Plan:拆步驟 — 轉譯與 AI 校正分離 + API 化(階段 1)

> 建立日:2026-07-24。狀態:**已實作,CLI 模式實測通過(2026-07-24)**。
> CLI 模式實測:30-Day_Day_1 集,57.8s,SPEAKER_02→Jenny 共 112 行(舊 fix_speakers 漏掉的),套用/相容/log 全正常。
> CLI 慢的原因:`claude -p` 仍載入完整 Claude Code 環境,單發呼叫省不掉冷啟動。
> **待辦:API 模式實測**——Console 買 credits 在原瀏覽器卡 Stripe 表單(按鈕灰),Safari 可以,key 還沒開。
> 拿到 key 後:設定切 API 模式 → 對同一集「重新校正」→ 比對 log 耗時(預估 3~5s vs CLI 57.8s)。
> 後續追加的 UI:校正獨立分頁、設定 iOS switch + 分頁範圍設定、密碼眼睛、刪除(dialog 確認)、Rust 端 log。
> 這是「給別人用 / 履歷專案」重構的第一階段。整體 roadmap 見文末。

## 背景與動機

現況 [scripts/transcribe.py](../../scripts/transcribe.py) 把兩件事綁在一條管線:

1. **轉譯**:whisperx(ASR + 對齊 + diarization)→ 產出 `word.txt`
2. **AI 校正**:shell out `claude` CLI(agentic loop,拿 Edit/Read/Write 工具直接改檔)把 `SPEAKER_XX` 換成真名

問題:

- CLI 校正走完整 agentic loop,**14~57 秒不穩定**(log 實測);單次 API 呼叫可壓到秒級
- `transcribe.py:115` **不檢查 fix_speakers 結果**,校正失敗照樣印 `DONE`
- CLI 依賴本機登入的 Claude 帳號,**給別人用完全不可行**
- 校正 in-place 覆寫 `word.txt`,**沒有乾淨輸入可重跑**
- 只改 speaker 名字,**不修文字內容**——但 mp3 轉譯後即刪除,逐字稿是唯一真相,whisper 聽錯的字 TTS 就照唸(使用者已確認此痛點)

## 目標設計

### 狀態機

```text
⬇ 下載 → ⚙ 轉譯 → 📝 已轉譯(即可練習)──[AI 校正,選配/手動/可重跑]──> ✨ 已校正
```

- 校正是**選配**:「已轉譯」就能練習,校正只是品質加值,AI 掛了不擋主流程
- 觸發是**手動**:清單上一顆「AI 校正」按鈕(API 計費,使用者自主)
- 校正**可重跑**:輸入永遠是 word.raw.txt

### 檔案策略(每個 podcast 資料夾)

| 檔案 | 產生者 | 說明 |
| --- | --- | --- |
| `word.raw.txt` | 轉譯 | 原始逐字稿,**永不修改** |
| `word.txt` | 轉譯(初始為 raw 的複本)→ 校正後覆寫 | 練習實際讀的檔 |
| `correction.json` | 校正 | API 回應存檔;**存在 = 已校正**(狀態判定依據) |

### 狀態判定

- 已轉譯 = `word.txt` 存在(不變,沿用現有邏輯)
- 已校正 = `correction.json` 存在
- 重跑校正 = 刪 `correction.json` 重按,或直接重按(覆寫)

## 設計變更(2026-07-24 實作時追加)

- **獨立「校正」分頁**:導覽列 下載|轉譯|校正|練習;下載頁的校正 UI 保留不動,兩邊共用同一狀態
- **雙模式校正**(使用者要求保留本機路徑):config `correction_mode` = `"api"`(預設)| `"cli"`
  - `api`:Anthropic API + structured outputs(給別人用的正路)
  - `cli`:本機 `claude -p` 單次呼叫(**無 agentic loop、不給工具**,同一套 prompt 回 JSON 本地套用,吃訂閱額度)——自用免費,發佈版之後可隱藏此選項
  - 兩路徑共用 prompt / JSON 解析(`extract_json` 容錯 markdown fence)/ 套用邏輯;`correction.json` 記錄 `mode`
  - Settings 有 radio 切換;校正頁顯示目前模式徽章

## AI 呼叫設計(Rust 端,取代 fix_speakers.py)

### 一次呼叫,兩個產出

- **speaker 對照**(誰是誰)+ **逐行修正**(只修明顯 ASR 錯字與標點)
- prompt 明確劃線:**不改語法、不刪贅字、不改寫句子、不合併/拆分行**——保住聽力素材保真
- 只回有問題的行,行號 1:1 對應(第一版不允許重新斷句)

### API 規格

- 端點:`POST https://api.anthropic.com/v1/messages`(Rust 無官方 SDK,reqwest 打 raw HTTP)
- Headers:`x-api-key`(從 config.json 讀)、`anthropic-version: 2023-06-01`、`content-type: application/json`
- 模型:`claude-haiku-4-5`(便宜、任務簡單;$1/$5 per MTok,一集逐字稿約 5~8k tokens,單次校正 < 0.01 USD)
- `max_tokens: 8192`(輸出只有對照表 + 錯誤行,足夠)
- **structured outputs**(`output_config.format` json_schema,Haiku 4.5 支援)強制回傳合法 JSON,免去 parse 失敗重試:

```json
{
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
```

> 註:schema 不支援數值範圍限制,`line` 有效性(1 ≤ line ≤ 行數)要本地驗證,越界的 fix 丟棄並記 log。
> 註:動態 key 的物件(`{"12": "..."}`)過不了 `additionalProperties: false`,所以用陣列格式。

### 本地套用順序

1. 讀 `word.raw.txt` 為行陣列(1-based)
2. 套 `fixes`(驗證行號範圍)
3. 套 `speakers`(替換行首 `[SPEAKER_XX]:` 前綴)
4. 寫 `word.txt` + `correction.json`
5. 任一步失敗 → 不動 `word.txt`,回傳錯誤給前端

## 工作分解

### A. Python

- [ ] `transcribe.py`:移除 Step 4(fix_speakers 呼叫);Step 2 改為產出 `word.raw.txt` 並複製一份為 `word.txt`
- [ ] 刪除 `scripts/fix_speakers.py`

### B. Rust([app/src-tauri/src/lib.rs](../../app/src-tauri/src/lib.rs))

- [ ] `Cargo.toml` 加 `reqwest`(`json` feature;tokio/serde 已有)
- [ ] 新 command `correct_transcript(folder)`:讀 raw → API → 本地套用 → 寫檔;完整錯誤處理(HTTP 4xx/5xx、網路錯誤、行號越界)
- [ ] `PodcastInfo` 加 `corrected: bool`(`correction.json` 存在)
- [ ] **修 practice.py 的 speaker regex**:`\[(\w+)\]:` 不接受含空白的名字(如 `Mary Ann`),改 `\[([^\]]+)\]:`——否則校正後名字帶空白會整行 parse 失敗(這個 bug 現在的 CLI 版就潛在存在)

### C. 前端

- [ ] [Settings.jsx](../../app/src/Components/Settings.jsx):加 Anthropic API Key 欄位(存 config.json,已 gitignore,與 hf_token 同模式;keychain 留到階段 4)
- [ ] 已轉譯清單項目:「AI 校正」按鈕 + 校正中 spinner + 已校正徽章;沒 API key 時按鈕 disabled 並提示去設定
- [ ] 校正失敗顯示錯誤,不改變項目狀態

## 驗收條件

1. 轉譯完成後,**不跑校正即可練習**
2. 校正單次呼叫完成,體感秒級(vs 現在 14~57s)
3. 校正失敗時 `word.txt` 不變、狀態不變、錯誤有顯示
4. 校正可重跑,結果冪等(同輸入近似同輸出)
5. 校正後 speaker 顯示真名、TTS 分聲正常(含帶空白名字)
6. 舊資料相容:既有資料夾只有 `word.txt` 沒有 `word.raw.txt` → 視為已轉譯未校正,校正前先把 `word.txt` 複製為 `word.raw.txt`

## 開放問題(不擋實作,做的時候順手決定)

- **mp3 要不要繼續刪?** 留著可供未來重轉譯(路線 B 換引擎後)或原音對照練習;代價是一集幾十 MB
- 斷句重切:第一版明確不做(行號 1:1 是省成本格式的前提),之後有需求再當獨立功能

## 整體 Roadmap(背景脈絡)

| 階段 | 內容 | 狀態 |
| --- | --- | --- |
| **1. 拆步驟 + AI 校正 API 化** | 本文件 | 完成 ✅ |
| 2. kokoro TTS → ONNX(**官方 sherpa-onnx crate**) | practice.py 退場 | **實作完成 ✅(2026-07-27)** |
| 3. ASR → **whisper-rs**(Metal);diarization → sherpa-onnx crate | 最大塊;**免 HF token** | **實作完成 ✅(2026-07-24)** |

### 階段 2 實作紀錄(2026-07-27)

- `app/src-tauri/src/native_tts.rs`:kokoro-multi-lang-v1_0(sherpa-onnx OfflineTts,models/ 內 393MB)取代 `scripts/practice.py`;`start_practice`(載入引擎)/`stop_practice`(釋放)/`play_line`(合成)三個 command 原地換掉,**前端零改動**(JSON 合約沿用:speaker/text/audio(b64 wav 24kHz)/index/total)
- VOICE_POOL 八聲輪替與 Python 版一致,名字→sid 對照:af_heart=3, am_adam=11, af_bella=2, am_michael=16, af_sky=10, am_echo=12, af_nova=7, am_liam=15(v1_0 speaker id 表)
- headless 實測(`native_test tts <folder> <index> <out.wav>`):**模型載入 0.7s**(Python KPipeline 要數秒)、4.8s 句子合成 2.1s
- G2P 差異註記:espeak-ng(sherpa)vs misaki(Python),數字/縮寫唸法可能微差,實聽驗收
- **聲音性別分配(2026-07-27 追加)**:舊輪替制性別盲(Day 2 James 抽到女聲)。三層選聲:每集 `voices.json` 手動指定(練習卡 🔊 鈕 → VoiceDialog,12 個美音)> 校正 structured output 新增 `gender`(m/f/u,存 correction.json)→ 男/女聲池輪流 > 舊輪替 fallback。基頻實測驗證(男 ~120Hz/女 ~200Hz)。舊 correction.json 沒 gender,「重新校正」即補上
- **diarization threshold 定案 1.1(2026-07-28)**:Day 3(角色扮演型內容)在 1.0 切出 6 標籤,1.1 收斂成 3(262/201/5);Day 2 於 1.0/1.1 皆乾淨、1.2 崩單人 → 預設值改 1.1。校正 prompt 同步強化:要求每個標籤都回報(含 gender),且明顯是既有講者的殘留小標籤要併回去
- 同時清掉 HF Token UI(Settings transcribe 區塊 + 轉譯頁設定鈕);config 的 hf_token 鍵不再讀寫
- **Python 現況**:`practice.py`、`transcribe.py` 已無人呼叫(保留當 rollback,穩定後可刪);仍在用的只剩 `download.py`(yt-dlp 包裝)
- 附帶(同日稍早):校正/轉譯狀態全域化(CorrectionProvider + correction-done 事件)、Rust TaskGuard 防重複觸發、download.py 修 YouTube 403(player_client=default)+ 失敗清空資料夾

### 階段 3 實作紀錄(2026-07-24)

- `app/src-tauri/src/native_transcribe.rs`:完整原生管線(ffmpeg 轉 16k wav → whisper 逐詞 → diarization → 逐詞掛講者 + 視窗多數決平滑 → 句級斷行 → word.raw.txt + word.txt)
- `transcribe_audio` command 改指向原生版,**前端零改動**(沿用 `transcribe-progress` 事件,現在有真實百分比);`scripts/transcribe.py` 保留但已不被呼叫(rollback 用,穩定後可刪)
- headless 測試:`cargo run --release --bin native_test <folder>`(src/bin/native_test.rs)
- 端對端實測(Day 2 副本):**總耗時 199.4s vs whisperx 894.6s(4.5x)**,361 行,講者爭議 1.9%,與 spike 一致
- config 新增可選 `diarization_threshold`(預設 1.0)
- **HF token 不再需要**(轉譯設定裡的 HuggingFace Token 欄位已無作用,UI 待清)
- 殘留待辦:diarization 試 model.int8.onnx 提速、平滑濾波對邊界 2 詞連跳無效(數字與未平滑相同,之後再調)、模型下載器(階段 4)

### 階段 3 spike 實測(2026-07-24,Day 2 集,994s 音檔)

- spike crate 在 `spike/`(獨立於 app),模型在 `models/`(皆已 gitignore);比對腳本在 session scratchpad `compare.py`
- **速度**:whisper.cpp(Metal, large-v3-turbo q5_0)轉譯 **105s(9.5x 即時)** vs whisperx **894.6s** → 快 8.5 倍;diarization 127~158s(現為瓶頸,threads 加了沒明顯改善);全管線 233s vs 895s ≈ **3.8x**
- **文字品質**:詞級相似度 0.989;**turbo 比 baseline(whisperx 預設 small)更好**——撿回 baseline 漏掉的兩整段、「Ginny」→正確的「Jenny」;差異僅 8 處(含 ten/10 等 cosmetic)
- **講者歸屬**:段落級分配 3.7% 詞掛錯(全在換人邊界);**word 模式(token timestamps + 句尾標點斷行)降到 1.9%**,且只剩散落邊界詞,無整句掛錯
- **diarization 調參**:titanet_small + FastClustering,threshold 預設 0.5 會爆 32 clusters;**threshold 1.0~1.1 → 3 位(接近真實 2)**;1.2 或 num_clusters=2 會崩成 1 位(勿用);過度切分經多數決映射後無害,但生產環境要餵 AI 校正,cluster 少較穩
- **生產版待辦**:word-speaker 序列平滑濾波(孤立詞跟隨前後,收掉碎行)、diarization 提速(試 model.int8.onnx)、threshold 進 config
- **結論:品質不輸反贏,速度大勝——階段 3 可行性確認 ✅**

### 原生化調查結論(2026-07-24)

- **sherpa-rs(社群 binding)已封存**——sherpa-onnx 官方出了 Rust API,直接用官方的
- 官方 `sherpa-onnx` crate:1.13.4(2026-07-08),與主專案同版號發版;OfflineTts 支援 kokoro;diarization 用 pyannote-segmentation-3.0 ONNX(GitHub releases 下載,**不經 HF、免 token**);macOS arm64 prebuilt + 靜態連結
- **ASR 不要用 sherpa-onnx**:whisper 錯字率高於 faster-whisper(issue #2900)、macOS CoreML 比 CPU 慢(issue #2910)
- **ASR 用 whisper-rs**(whisper.cpp binding):活躍維護(2026-03)、`metal` feature 即有 Metal 加速,M 系列 large-v3 約 10x 即時速
- 最終架構:ASR = whisper-rs(Metal)/ diarization + TTS = sherpa-onnx 官方 crate(CPU 即可,模型輕)
| 4. 路徑改 Application Support、打包、簽名公證、CI | 發佈基礎建設 | 進行中(1/5) |

### 階段 4 進度(2026-07-28)

- ✅ **步驟 1:下載去 Python 化**——`native_download.rs` 直接 spawn yt-dlp(fetch_title + run_download,含 clean_title 移植、進度事件、失敗清資料夾);**app 的 Python 依賴歸零**(scripts/ 三支全數退役,保留當 rollback;venv 可擇期刪除,start.command 的 activate 行已無作用)
- ✅ **步驟 2:路徑抽象**——`data_dir()`(OnceLock):debug 建置 = 專案根(開發資料不搬家)、release = `~/Library/Application Support/com.allenming.english-practice`;`init_data_dir()` 供 native_test 覆寫;`find_tool()` 統一找外部 CLI(PATH + Homebrew arm64/Intel 路徑)
- ✅ **步驟 3:模型下載器**——`native_models.rs`:四模型 manifest(whisper/segmentation/titanet/kokoro,共 ~1GB 下載量)、reqwest 串流 + 進度事件(model-progress,每 3MB)、tar.bz2 用 /usr/bin/tar 解(零新 crate)、marker 檔驗證;前端 `ModelSetup.jsx` 啟動檢查彈窗(App.jsx 掛載,模型齊全不顯示);headless 實測通過(File + TarBz2 兩路徑)
- ✅ **步驟 4:外部工具引導**——`tools_status` command + Download 頁缺件 banner(顯示 brew install 指令);「內嵌 binary」留到打包時再評估
- 待辦:⑤ tauri build + 簽名公證(需 Apple Developer 帳號,使用者暫緩;無帳號替代:發佈附 SHA256 checksum)+ CI

### 安全補強(2026-07-28)

- **API key → macOS Keychain**:經 `/usr/bin/security` CLI(零新依賴;accessor 是 Apple 簽名工具,dev 重編不觸發重授權)。`save_config` 攔截 `anthropic_api_key` 入 Keychain(service `com.allenming.english-practice`),`get_config`/校正走 `load_config_merged()` 疊回;檔案內舊明文 key 首次讀取自動搬遷。config.json 同時 chmod 600,已清掉無用的 hf_token 與空 key
- **模型 SHA256 驗證**:ModelSpec 加 `sha256`(驗 marker 檔,tar 解開後驗;不符即清除落地物);sha2 crate
- **validate_folder()**:防路徑跳脫,掛上所有收 folder 的 command(先前只有 delete_podcast 有擋)
- **CSP**:tauri.conf.json 從 null 改為嚴格白名單(media-src data:/blob: 供 TTS 音訊、ws://localhost:5173 供 dev HMR)
- **依賴稽核**:npm audit fix 後 vite 8.1.5;殘留 7 項評估為不適用(brace-expansion 在 eslint 樹內=dev only;react-router CSRF 僅 RSC/server actions 模式,本地 SPA 無此面)。cargo-audit(--locked 安裝):quick-xml 7.5 high ×2,tauri→plist 釘版無法本地升,且無不可信 XML 輸入路徑,記錄待 tauri 上游;22 條 unmaintained 警告多為 Linux GTK 綁定(macOS 未用)
- 已知取捨:security CLI 寫入瞬間 key 現於 argv(本機單人可接受);正式簽名後可改 Security.framework(keyring crate)消除
- 附帶:AI 校正 prompt 改為「每位講者都回報一筆(含 gender)」——修正已具名講者時 speakers 回空陣列、gender 記不到的問題

決策紀錄:目標是**履歷作品**;平台先 macOS(Apple Silicon);走原生路線(路線 B)已傾向確定,階段 1 完成後依 Rust 手感再確認階段 3 引擎選擇。
