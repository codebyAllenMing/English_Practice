from core import env_setup  # noqa: F401
import os
import sys
import json
import time
import shutil
import subprocess
from core.logger import log_info, log_error, log_exception

PODCASTS_DIR = "podcasts"
CONFIG_FILE = "config.json"


def load_config():
    if not os.path.exists(CONFIG_FILE):
        return {}
    with open(CONFIG_FILE, "r") as f:
        return json.load(f)


def transcribe(folder_name):
    start_time = time.time()
    log_info("transcribe", f"開始轉譯: {folder_name}")

    config = load_config()
    hf_token = config.get("hf_token", "")

    if not hf_token:
        log_error("transcribe", "未設定 HuggingFace Token")
        print("ERROR:請先設定 HuggingFace Token", flush=True)
        sys.exit(1)

    folder_path = os.path.join(PODCASTS_DIR, folder_name)
    audio_path = os.path.join(folder_path, "podcast.mp3")

    if not os.path.exists(audio_path):
        log_error("transcribe", f"找不到音檔: {audio_path}")
        print(f"ERROR:找不到音檔 {audio_path}", flush=True)
        sys.exit(1)

    # Step 1: whisperx 轉譯
    whisperx_start = time.time()
    log_info("transcribe", f"[{folder_name}] whisperx 開始")
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
    whisperx_elapsed = time.time() - whisperx_start

    if proc.returncode != 0:
        detail = "\n".join(output_lines[-20:])
        log_error("transcribe", f"[{folder_name}] whisperx 失敗 (耗時 {whisperx_elapsed:.1f}s)", detail)
        print("ERROR:轉譯失敗", flush=True)
        sys.exit(1)

    log_info("transcribe", f"[{folder_name}] whisperx 完成 (耗時 {whisperx_elapsed:.1f}s)")

    # Step 2: 產生 word.txt
    json_path = os.path.join(folder_path, "podcast.json")
    if not os.path.exists(json_path):
        log_error("transcribe", f"[{folder_name}] 找不到轉譯結果 JSON")
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

    # word.raw.txt 為原始逐字稿(永不修改),word.txt 為練習實際讀的檔
    # AI 校正(Rust 端 correct_transcript)以 raw 為輸入、覆寫 word.txt
    word_raw_path = os.path.join(folder_path, "word.raw.txt")
    word_path = os.path.join(folder_path, "word.txt")
    with open(word_raw_path, "w") as f:
        f.write("\n".join(lines))
    shutil.copyfile(word_raw_path, word_path)

    log_info("transcribe", f"[{folder_name}] word.raw.txt / word.txt 產生完成 ({len(lines)} 行)")

    # Step 3: 刪除 mp3
    os.remove(audio_path)
    if os.path.exists(json_path):
        os.remove(json_path)

    # 總結
    total_elapsed = time.time() - start_time
    log_info("transcribe", f"[{folder_name}] 全部完成 (總耗時 {total_elapsed:.1f}s, whisperx {whisperx_elapsed:.1f}s)")

    print(f"DONE:{word_path}", flush=True)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("用法：python3 -m scripts.transcribe <folder_name>")
        sys.exit(1)

    transcribe(sys.argv[1])
