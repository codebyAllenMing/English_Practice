from core import env_setup  # noqa: F401
import sys
import time
import subprocess
from core.logger import log_info, log_error, log_exception


def fix_speakers(word_path):
    start_time = time.time()
    log_info("fix_speakers", f"開始校正: {word_path}")

    try:
        result = subprocess.run([
            "claude", "-p",
            f"讀取 {word_path}，把 SPEAKER_XX 替換成真實名字（從對話中判斷如 'My name is X'）。無法判斷的保留。直接修改檔案。",
            "--allowedTools", "Edit,Read,Write",
            "--model", "haiku",
            "--no-session-persistence",
        ], timeout=300, capture_output=True, text=True)

        elapsed = time.time() - start_time

        if result.returncode != 0:
            log_error("fix_speakers", f"Claude CLI 失敗 (耗時 {elapsed:.1f}s)", result.stderr[:500])
            print("ERROR:校正失敗", flush=True)
            return

        log_info("fix_speakers", f"校正完成: {word_path} (耗時 {elapsed:.1f}s)")
        print(f"DONE:校正完成 ({elapsed:.1f}s)", flush=True)

    except subprocess.TimeoutExpired:
        elapsed = time.time() - start_time
        log_error("fix_speakers", f"Claude CLI 逾時 (耗時 {elapsed:.1f}s)")
        print("ERROR:校正逾時", flush=True)
    except FileNotFoundError:
        log_error("fix_speakers", "找不到 claude CLI")
        print("ERROR:找不到 claude CLI", flush=True)
    except Exception:
        log_exception("fix_speakers")
        print("ERROR:校正時發生錯誤", flush=True)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("用法：python3 -m scripts.fix_speakers <word.txt 路徑>")
        sys.exit(1)

    fix_speakers(sys.argv[1])
