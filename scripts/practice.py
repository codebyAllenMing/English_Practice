from core import env_setup  # noqa: F401
import re
import os
import sys
import json
import base64
import io
import numpy as np
import soundfile as sf
from kokoro import KPipeline

PODCASTS_DIR = "podcasts"

pipeline = KPipeline(lang_code='a')
VOICE_POOL = [
    "af_heart", "am_adam", "af_bella", "am_michael",
    "af_sky", "am_echo", "af_nova", "am_liam",
]
voice_map = {}

print("READY", flush=True)


def get_voice(speaker):
    if speaker not in voice_map:
        voice_map[speaker] = VOICE_POOL[len(voice_map) % len(VOICE_POOL)]
    return voice_map[speaker]


def generate_audio(folder_name, line_index):
    word_path = os.path.join(PODCASTS_DIR, folder_name, "word.txt")
    with open(word_path, "r") as f:
        lines = [line.strip() for line in f.readlines() if line.strip()]

    if line_index < 0 or line_index >= len(lines):
        print("ERROR:無效的行數", flush=True)
        return

    line = lines[line_index]
    # [^\]]+ 而非 \w+:校正後的名字可能含空白(如 Mary Ann)
    match = re.match(r'\[([^\]]+)\]:\s*(.*)', line)
    if match:
        speaker = match.group(1)
        text = match.group(2)
    else:
        speaker = "UNKNOWN"
        text = line

    voice = get_voice(speaker)

    audio_parts = []
    for _, _, audio in pipeline(text, voice=voice):
        audio_parts.append(audio)

    if not audio_parts:
        print("ERROR:無法產生音訊", flush=True)
        return

    audio = np.concatenate(audio_parts)
    buf = io.BytesIO()
    sf.write(buf, audio, 24000, format='WAV')
    b64 = base64.b64encode(buf.getvalue()).decode()

    result = {
        "speaker": speaker,
        "text": text,
        "audio": b64,
        "index": line_index,
        "total": len(lines),
    }
    print(f"RESULT:{json.dumps(result)}", flush=True)


# 主迴圈：從 stdin 讀取指令
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    if line == "QUIT":
        break
    try:
        cmd = json.loads(line)
        generate_audio(cmd["folder"], cmd["index"])
    except Exception as e:
        print(f"ERROR:{e}", flush=True)
