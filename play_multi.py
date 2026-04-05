import re
import sys
import tty
import termios
import os
import sounddevice as sd
from kokoro import KPipeline

PODCASTS_DIR = "podcasts"

# 自動分配用的聲音池
VOICE_POOL = [
    "af_heart", "am_adam", "af_bella", "am_michael",
    "af_sky", "am_echo", "af_nova", "am_liam",
]
_auto_voice_map = {}

def get_voice(speaker):
    if speaker not in _auto_voice_map:
        _auto_voice_map[speaker] = VOICE_POOL[len(_auto_voice_map) % len(VOICE_POOL)]
    return _auto_voice_map[speaker]

pipeline = KPipeline(lang_code='a')

# 選擇 podcast
podcasts = sorted([
    d for d in os.listdir(PODCASTS_DIR)
    if os.path.isdir(os.path.join(PODCASTS_DIR, d))
    and os.path.exists(os.path.join(PODCASTS_DIR, d, "word.txt"))
])

if not podcasts:
    print("找不到任何 podcast！")
    sys.exit(1)

def choose_podcast(podcasts):
    cursor = 0

    def render():
        sys.stdout.write("\033[H\033[J")  # 清空畫面
        sys.stdout.write("選擇 podcast（↑↓ 移動，Enter 確認）：\r\n\r\n")
        for i, name in enumerate(podcasts):
            prefix = "▶ " if i == cursor else "  "
            sys.stdout.write(f"  {prefix}{name}\r\n")
        sys.stdout.flush()

    fd = sys.stdin.fileno()
    old = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        render()
        while True:
            ch = sys.stdin.read(1)
            if ch == '\x1b':
                ch2 = sys.stdin.read(1)
                ch3 = sys.stdin.read(1)
                if ch2 == '[':
                    if ch3 == 'A':  # 上
                        cursor = max(0, cursor - 1)
                    elif ch3 == 'B':  # 下
                        cursor = min(len(podcasts) - 1, cursor + 1)
                render()
            elif ch in ('\r', '\n'):
                return podcasts[cursor]
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)

selected = choose_podcast(podcasts)

TEXT_FILE = os.path.join(PODCASTS_DIR, selected, "word.txt")
print(f"\n播放：{selected}\n")

with open(TEXT_FILE, "r") as f:
    lines = [line.strip() for line in f.readlines() if line.strip()]

def read_key():
    fd = sys.stdin.fileno()
    old = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        ch = sys.stdin.read(1)
        if ch == '\x1b':
            ch2 = sys.stdin.read(1)
            ch3 = sys.stdin.read(1)
            if ch2 == '[' and ch3 == 'A':
                return 'up'
            if ch2 == '[' and ch3 == 'B':
                return 'down'
            if ch2 == '[' and ch3 == 'D':
                return 'left'
            return 'other'
        return ch
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)

def play_line(i):
    line = lines[i]
    match = re.match(r'\[(\w+)\]:\s*(.*)', line)
    if match:
        speaker = match.group(1)
        text = match.group(2)
    else:
        speaker = '?'
        text = line
    voice = get_voice(speaker)

    print(f"[{i+1}/{len(lines)}] [{speaker}] {text}")
    generator = pipeline(text, voice=voice)
    for _, _, audio in generator:
        sd.play(audio, samplerate=24000)
        sd.wait()

print("空白鍵/↓：下一句　↑：上一句　←：重複　q：離開\n")

i = 0
while i < len(lines):
    play_line(i)
    key = read_key()
    if key == 'up':
        i = max(0, i - 1)
    elif key == 'left':
        pass  # 重複當前句，i 不變
    elif key == 'q':
        break
    else:
        i += 1
