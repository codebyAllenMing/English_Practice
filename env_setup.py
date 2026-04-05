import os

PROJECT_DIR = os.path.dirname(os.path.abspath(__file__))
VENV_BIN = os.path.join(PROJECT_DIR, "venv", "bin")
HOMEBREW_BIN = "/opt/homebrew/bin"

# 把 venv 和 homebrew 加到 PATH，確保所有工具都找得到
os.environ["PATH"] = f"{VENV_BIN}:{HOMEBREW_BIN}:{os.environ.get('PATH', '')}"
