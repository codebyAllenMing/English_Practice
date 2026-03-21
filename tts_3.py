import re
import tty
import termios
import sys
import sounddevice as sd
from kokoro import KPipeline

# 設定
TEXT_FILE = "Career Strategy For People With Too Many Interests/word.txt"

# 角色對應聲音
VOICE_MAP = {
    "Jake": "am_michael",
    "Anna": "af_heart",
}
DEFAULT_VOICE = "af_heart"

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
        voice = VOICE_MAP.get(speaker, DEFAULT_VOICE)
    else:
        text = line
        voice = DEFAULT_VOICE

    print(f"[{i+1}/{len(lines)}] [{speaker if match else '?'}] {text}")

    generator = pipeline(text, voice=voice)
    for _, _, audio in generator:
        sd.play(audio, samplerate=24000)
        sd.wait()

    wait_for_space()