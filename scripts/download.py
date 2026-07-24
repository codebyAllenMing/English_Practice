from core import env_setup  # noqa: F401
import re
import os
import sys
import time
import subprocess
from core.logger import log_info, log_error, log_exception

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


def fetch_title(url):
    """只取得標題與建議的資料夾名稱，不下載"""
    title = get_video_title(url)
    folder_name = clean_title(title)
    print(f"TITLE:{title}", flush=True)
    print(f"FOLDER:{folder_name}", flush=True)


def download(url, folder_name=None):
    start_time = time.time()
    log_info("download", f"開始下載: {url}")

    try:
        if folder_name is None:
            title = get_video_title(url)
            folder_name = clean_title(title)
        else:
            title = folder_name

        folder_path = os.path.join(PODCASTS_DIR, folder_name)

        if os.path.exists(folder_path):
            log_error("download", f"資料夾已存在: {folder_name}")
            print(f"ERROR:資料夾已存在：{folder_name}", flush=True)
            sys.exit(1)

        os.makedirs(folder_path)
        log_info("download", f"標題: {title}, 資料夾: {folder_name}")

        output_path = os.path.join(folder_path, "podcast.mp3")
        print(f"TITLE:{title}", flush=True)
        print(f"FOLDER:{folder_name}", flush=True)

        proc = subprocess.Popen(
            ["yt-dlp", "-x", "--audio-format", "mp3",
             "--ffmpeg-location", "/opt/homebrew/bin",
             # android vr client 會被 YouTube 回 403(2026-07 實測),固定用 default client
             "--extractor-args", "youtube:player_client=default",
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
        elapsed = time.time() - start_time

        if proc.returncode != 0:
            detail = "\n".join(output_lines[-10:])
            log_error("download", f"yt-dlp 下載失敗 (耗時 {elapsed:.1f}s): {url}", detail)
            # 失敗時清掉剛建立的空資料夾,避免重試撞「資料夾已存在」
            if os.path.isdir(folder_path) and not os.listdir(folder_path):
                os.rmdir(folder_path)
            print("ERROR:下載失敗", flush=True)
            sys.exit(1)

        log_info("download", f"下載完成: {folder_name} (耗時 {elapsed:.1f}s)")
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
        print("用法：python3 -m scripts.download <URL>")
        print("      python3 -m scripts.download --fetch-title <URL>")
        print("      python3 -m scripts.download <URL> <folder_name>")
        sys.exit(1)

    if sys.argv[1] == "--fetch-title":
        if len(sys.argv) < 3:
            print("用法：python3 -m scripts.download --fetch-title <URL>")
            sys.exit(1)
        fetch_title(sys.argv[2])
    else:
        url = sys.argv[1]
        folder = sys.argv[2] if len(sys.argv) > 2 else None
        download(url, folder)
