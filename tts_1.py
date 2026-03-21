import soundfile as sf
import sounddevice as sd
import numpy as np
from kokoro import KPipeline
import re
    
# 設定
TEXT_FILE = "Career Strategy For People With Too Many Interests/word.txt"  # 換成你的檔名
VOICE = "af_sky"           # 換成你想要的聲音

pipeline = KPipeline(lang_code='a')

with open(TEXT_FILE, "r") as f:
    lines = [re.sub(r'\[.*?\]:\s*', '', line.strip()) for line in f.readlines() if line.strip()]

for i, line in enumerate(lines):
    print(f"[{i+1}/{len(lines)}] {line}")
    generator = pipeline(line, voice=VOICE)
    for _, _, audio in generator:
        sd.play(audio, samplerate=24000)
        sd.wait()