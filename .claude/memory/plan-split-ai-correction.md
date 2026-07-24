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
| **1. 拆步驟 + AI 校正 API 化** | 本文件 | ← 現在 |
| 2. kokoro TTS → ONNX(sherpa-rs) | 第一次碰 FFI,範圍小 | 待 sherpa-rs 成熟度調查 |
| 3. ASR + diarization → 原生(whisper.cpp / sherpa-onnx) | 最大塊;Metal 加速可大幅縮短 15~22 分鐘的轉譯時間;可能免去 HF token | 同上 |
| 4. 路徑改 Application Support、打包、簽名公證、CI | 發佈基礎建設 | |

決策紀錄:目標是**履歷作品**;平台先 macOS(Apple Silicon);走原生路線(路線 B)已傾向確定,階段 1 完成後依 Rust 手感再確認階段 3 引擎選擇。
