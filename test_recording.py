import tty
import termios
import sys
import os
import threading
import time
import sounddevice as sd
import soundfile as sf
import numpy as np
from datetime import datetime
from faster_whisper import WhisperModel

SAMPLE_RATE = 16000
CHANNELS = 1
CHUNK_SECONDS = 3
OUTPUT_DIR = "voices_recordings"

model = WhisperModel("small", device="cpu", compute_type="int8")

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

TEST_TEXT = "My name is Anna and I enjoy learning English every day."
print(f"\n{TEST_TEXT}\n")
print("按空白鍵開始錄音...")
wait_for_space()

frames = []
stop_flag = threading.Event()

def callback(indata, frame_count, time_info, status):
    frames.append(indata.copy())

SILENCE_THRESHOLD = 0.03
SILENCE_DURATION = 1.0  # 停頓超過 1 秒才送去辨識

def rms(audio):
    return np.sqrt(np.mean(audio ** 2))

def set_status(msg):
    print(f"\r\033[K{msg}", end="", flush=True)

def transcribe_loop():
    processed = 0
    speech_frames = []
    silence_since = None
    state = None

    set_status("[等待中...]")

    while not stop_flag.is_set():
        time.sleep(0.1)
        current = len(frames)
        if current <= processed:
            continue

        new_audio = np.concatenate(frames[processed:current], axis=0)
        processed = current
        level = rms(new_audio)

        if level >= SILENCE_THRESHOLD:
            if state != "speaking":
                set_status("[說話中...]")
                state = "speaking"
            speech_frames.append(new_audio)
            silence_since = None
        elif not speech_frames:
            if state != "waiting":
                set_status("[等待中...]")
                state = "waiting"
            frames.clear()
            processed = 0
        elif speech_frames:
            if silence_since is None:
                silence_since = time.time()
                set_status("[偵測到停頓，等待...]")
            elif time.time() - silence_since >= SILENCE_DURATION:
                set_status("[辨識中...]")
                state = None
                chunk = np.concatenate(speech_frames, axis=0)
                speech_frames = []
                silence_since = None
                tmp_path = os.path.join(OUTPUT_DIR, "_tmp_chunk.wav")
                sf.write(tmp_path, chunk, SAMPLE_RATE)
                segments, _ = model.transcribe(tmp_path, language="en")
                text = " ".join(s.text.strip() for s in segments)
                if text:
                    print(f"\r\033[K> {text}\n", flush=True)
                set_status("[等待中...]")

print("錄音中... 按空白鍵停止")
with sd.InputStream(samplerate=SAMPLE_RATE, channels=CHANNELS, callback=callback):
    t = threading.Thread(target=transcribe_loop, daemon=True)
    t.start()
    wait_for_space()
    stop_flag.set()

# 儲存完整錄音
audio = np.concatenate(frames, axis=0)
filename = datetime.now().strftime("%Y%m%d_%H%M%S") + ".wav"
filepath = os.path.join(OUTPUT_DIR, filename)
sf.write(filepath, audio, SAMPLE_RATE)
print(f"\n\n已儲存：{filepath}")
