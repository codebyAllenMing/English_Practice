import re
import sounddevice as sd
from kokoro import KPipeline

# 設定
TEXT_FILE = "Career Strategy For People With Too Many Interests/word.txt"

# 角色對應聲音
VOICE_MAP = {
    "Jake": "am_adam",
    "Anna": "af_bella",
}
DEFAULT_VOICE = "af_heart"  # 沒有標註說話者時的預設聲音

pipeline = KPipeline(lang_code='a')

with open(TEXT_FILE, "r") as f:
    lines = [line.strip() for line in f.readlines() if line.strip()]

for i, line in enumerate(lines):
    # 抓出說話者
    match = re.match(r'\[(\w+)\]:\s*(.*)', line)
    if match:
        speaker = match.group(1)
        text = match.group(2)
        voice = VOICE_MAP.get(speaker, DEFAULT_VOICE)
    else:
        text = line
        voice = DEFAULT_VOICE

    print(f"[{i+1}/{len(lines)}] [{speaker if match else '?'}] {text}")
    
    generator = pipeline(text, voice=voice)
    for _, _, audio in generator:
        sd.play(audio, samplerate=24000)
        sd.wait()