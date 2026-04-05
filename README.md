# English Practice

用 YouTube Podcast 練習英文聽力與口說的工具。

## 環境需求

- Python 3.11+
- macOS（使用 `termios` 鍵盤控制）
- ffmpeg（`brew install ffmpeg`）

## 安裝

```bash
python3 -m venv venv
source venv/bin/activate
pip install kokoro sounddevice faster-whisper whisperx
```

## 使用流程

### Step 1：下載 YouTube 音檔

```bash
yt-dlp -x --audio-format mp3 -o "podcast.mp3" "https://www.youtube.com/watch?v=XXXX"
```

### Step 2：轉譯文字（含說話者辨識）

```bash
export HF_TOKEN="你的 HuggingFace token"

whisperx "podcast.mp3" --language en --diarize --hf_token $HF_TOKEN --output_dir ./output
```

輸出格式：
```
[SPEAKER_00]: This is your everyday English class.
[SPEAKER_01]: Hello, welcome to the show.
```

> HuggingFace token 到 https://huggingface.co/settings/tokens 申請

### Step 3：建立 podcast 資料夾

將轉譯好的文字存到 `podcasts/` 下：

```
podcasts/
  my-podcast/
    word.txt      ← 放轉譯後的文字
```

### Step 4：播放練習

```bash
python3 play_multi.py
```

操作方式：
- **↑↓** 選擇 podcast，**Enter** 確認
- **空白鍵 / ↓** 下一句
- **↑** 上一句
- **←** 重複當前句
- **q** 離開

每個 `[SPEAKER_XX]` 會自動分配不同語音。

### Step 5：口說練習（選用）

```bash
python3 test_recording.py
```

- **空白鍵** 開始 / 停止錄音
- 即時語音辨識，偵測到停頓後自動轉文字
- 錄音檔儲存在 `voices_recordings/`

## 專案結構

```
English_Practice/
  play_multi.py         ← 多角色語音播放
  play_mono.py          ← 單一語音播放
  play_interactive.py   ← 互動式播放
  test_recording.py     ← 口說錄音 + 即時辨識
  podcasts/             ← podcast 文字檔
  voices_recordings/    ← 錄音檔
  VOICES.md             ← 可用語音列表
```
