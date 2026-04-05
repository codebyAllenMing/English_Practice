import env_setup
import re
import os
import sys
import json
import subprocess
from logger import log_error, log_exception

PODCASTS_DIR = "podcasts"
CONFIG_FILE = "config.json"


def load_config():
    if not os.path.exists(CONFIG_FILE):
        return {}
    with open(CONFIG_FILE, "r") as f:
        return json.load(f)


def detect_speaker_names(lines):
    """從對話中抓 'My name is X' 或 'I'm X' 來對應說話者"""
    patterns = [
        r"[Mm]y name is (\w+)",
        r"I'm (\w+)",
        r"I am (\w+)",
        r"[Cc]all me (\w+)",
    ]
    speaker_map = {}
    for line in lines:
        match_speaker = re.match(r'\[(\w+)\]:\s*(.*)', line)
        if not match_speaker:
            continue
        speaker = match_speaker.group(1)
        text = match_speaker.group(2)
        if speaker in speaker_map:
            continue
        for pattern in patterns:
            name_match = re.search(pattern, text)
            if name_match:
                name = name_match.group(1)
                # 過濾掉太短或不像名字的
                if len(name) >= 2 and name[0].isupper():
                    speaker_map[speaker] = name
                    break
    return speaker_map


def transcribe(folder_name):
    config = load_config()
    hf_token = config.get("hf_token", "")

    if not hf_token:
        print("ERROR:請先設定 HuggingFace Token", flush=True)
        sys.exit(1)

    folder_path = os.path.join(PODCASTS_DIR, folder_name)
    audio_path = os.path.join(folder_path, "podcast.mp3")

    if not os.path.exists(audio_path):
        print(f"ERROR:找不到音檔 {audio_path}", flush=True)
        sys.exit(1)

    print("PROGRESS:開始轉譯...", flush=True)

    proc = subprocess.Popen(
        [
            "whisperx", audio_path,
            "--language", "en",
            "--diarize",
            "--hf_token", hf_token,
            "--output_dir", folder_path,
            "--output_format", "json",
        ],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
    )

    output_lines = []
    for line in proc.stdout:
        line = line.strip()
        output_lines.append(line)
        if "Performing voice activity detection" in line:
            print("PROGRESS:語音偵測中...", flush=True)
        elif "alignment" in line.lower():
            print("PROGRESS:對齊中...", flush=True)
        elif "diarization" in line.lower():
            print("PROGRESS:辨識說話者...", flush=True)
        elif "Transcribing" in line or "transcrib" in line.lower():
            print("PROGRESS:轉錄中...", flush=True)

    proc.wait()
    if proc.returncode != 0:
        detail = "\n".join(output_lines[-20:])
        log_error("transcribe", f"whisperx 轉譯失敗: {folder_name}", detail)
        print("ERROR:轉譯失敗", flush=True)
        sys.exit(1)

    # 讀取 whisperx JSON 輸出，轉成 word.txt
    json_path = os.path.join(folder_path, "podcast.json")
    if not os.path.exists(json_path):
        print("ERROR:找不到轉譯結果", flush=True)
        sys.exit(1)

    print("PROGRESS:產生文字檔...", flush=True)

    with open(json_path, "r") as f:
        data = json.load(f)

    lines = []
    for seg in data.get("segments", []):
        speaker = seg.get("speaker", "UNKNOWN")
        text = seg.get("text", "").strip()
        if text:
            lines.append(f"[{speaker}]: {text}")

    word_path = os.path.join(folder_path, "word.txt")
    with open(word_path, "w") as f:
        f.write("\n".join(lines))

    # 刪除 mp3
    os.remove(audio_path)
    if os.path.exists(json_path):
        os.remove(json_path)

    # 校正說話者名字
    print("PROGRESS:校正說話者名字...", flush=True)
    subprocess.run([sys.executable, "fix_speakers.py", word_path])

    print(f"DONE:{word_path}", flush=True)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("用法：python3 transcribe.py <folder_name>")
        sys.exit(1)

    transcribe(sys.argv[1])
