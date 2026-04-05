import env_setup
import sys
import subprocess
from logger import log_error


def fix_speakers(word_path):
    try:
        result = subprocess.run([
            "claude", "-p",
            f"讀取 {word_path}，根據對話內容判斷 SPEAKER_XX 對應的真實名字（例如從 'My name is Anna' 判斷），"
            f"將所有 SPEAKER_XX 替換成對應的名字。如果無法判斷就保留原本的 SPEAKER_XX。"
            f"同時修正明顯的文法或拼寫錯誤。直接修改檔案，不要輸出其他內容。",
            "--allowedTools", "Edit,Read,Write"
        ], timeout=300, capture_output=True, text=True)

        if result.returncode != 0:
            log_error("fix_speakers", f"Claude CLI 失敗: {word_path}", result.stderr)
            print("ERROR:校正失敗", flush=True)
            return

        print("DONE:校正完成", flush=True)
    except subprocess.TimeoutExpired:
        log_error("fix_speakers", f"Claude CLI 逾時: {word_path}")
        print("ERROR:校正逾時", flush=True)
    except FileNotFoundError:
        log_error("fix_speakers", "找不到 claude CLI，請確認已安裝")
        print("ERROR:找不到 claude CLI", flush=True)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("用法：python3 fix_speakers.py <word.txt 路徑>")
        sys.exit(1)

    fix_speakers(sys.argv[1])
