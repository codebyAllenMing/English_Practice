import re
import tty
import termios
import sys
import os
import questionary
import sounddevice as sd
from kokoro import KPipeline

# 選擇 podcast
PODCASTS_DIR = "podcasts"
folders = sorted([f for f in os.listdir(PODCASTS_DIR) if os.path.isdir(os.path.join(PODCASTS_DIR, f))])

choice = questionary.select(
    "選擇要播放的 podcast：",
    choices=folders
).ask()

TEXT_FILE = os.path.join(PODCASTS_DIR, choice, "word.txt")

# 自動分配聲音池（依出現順序輪流）
VOICE_POOL = ["am_michael", "af_heart"]
DEFAULT_VOICE = "af_heart"

speaker_voice_map = {}

pipeline = KPipeline(lang_code='a')

def wait_for_space():
    fd = sys.stdin.fileno()
    old = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        while True:
            ch = sys.stdin.read(1)
            if ch == ' ':
                return
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)

with open(TEXT_FILE, "r") as f:
    lines = [line.strip() for line in f.readlines() if line.strip()]

for i, line in enumerate(lines):
    match = re.match(r'\[(\w+)\]:\s*(.*)', line)
    if match:
        speaker = match.group(1)
        text = match.group(2)
        if speaker not in speaker_voice_map:
            speaker_voice_map[speaker] = VOICE_POOL[len(speaker_voice_map) % len(VOICE_POOL)]
        voice = speaker_voice_map[speaker]
    else:
        text = line
        voice = DEFAULT_VOICE

    print(f"[{i+1}/{len(lines)}] [{speaker if match else '?'}] {text}")

    generator = pipeline(text, voice=voice)
    for _, _, audio in generator:
        sd.play(audio, samplerate=24000)
        sd.wait()

    wait_for_space()