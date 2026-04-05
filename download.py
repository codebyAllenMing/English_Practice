import env_setup
import re
import os
import sys
import subprocess
import json
from logger import log_error, log_exception

PODCASTS_DIR = "podcasts"


def get_video_title(url):
    result = subprocess.run(
        ["yt-dlp", "--print", "title", "--no-download", url],
        capture_output=True, text=True
    )
    if result.returncode != 0:
        log_error("download", f"無法取得標題: {url}", result.stderr)
        print(f"ERROR:無法取得標題", flush=True)
        sys.exit(1)
    return result.stdout.strip()


def clean_title(title):
    title = re.sub(r'\|.*', '', title)
    title = re.sub(r'\(.*?\)', '', title)
    title = re.sub(r'\[.*?\]', '', title)
    title = title.strip()
    title = re.sub(r'[^\w\s-]', '', title)
    title = re.sub(r'\s+', '_', title)
    return title


def download(url):
    try:
        title = get_video_title(url)
        folder_name = clean_title(title)
        folder_path = os.path.join(PODCASTS_DIR, folder_name)

        if os.path.exists(folder_path):
            print(f"ERROR:資料夾已存在：{folder_name}", flush=True)
            sys.exit(1)

        os.makedirs(folder_path)

        output_path = os.path.join(folder_path, "podcast.mp3")
        print(f"TITLE:{title}", flush=True)
        print(f"FOLDER:{folder_name}", flush=True)

        proc = subprocess.Popen(
            ["yt-dlp", "-x", "--audio-format", "mp3",
             "--ffmpeg-location", "ffmpeg",
             "--newline", "-o", output_path, url],
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
        )

        output_lines = []
        for line in proc.stdout:
            line = line.strip()
            output_lines.append(line)
            match = re.search(r'(\d+(?:\.\d+)?)%', line)
            if match:
                print(f"PROGRESS:{match.group(1)}", flush=True)
            elif "[ExtractAudio]" in line:
                print("PROGRESS:converting", flush=True)

        proc.wait()
        if proc.returncode != 0:
            detail = "\n".join(output_lines[-10:])
            log_error("download", f"yt-dlp 下載失敗: {url}", detail)
            print("ERROR:下載失敗", flush=True)
            sys.exit(1)

        print(f"DONE:{output_path}", flush=True)
        return folder_path
    except SystemExit:
        raise
    except Exception:
        log_exception("download")
        print("ERROR:下載時發生錯誤", flush=True)
        sys.exit(1)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("用法：python3 download.py <YouTube URL>")
        sys.exit(1)

    download(sys.argv[1])
