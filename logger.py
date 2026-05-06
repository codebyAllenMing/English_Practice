import os
import traceback
from datetime import datetime

LOG_DIR = os.path.join(os.path.dirname(__file__), "logs")
INFO_FILE = os.path.join(LOG_DIR, "info.log")
ERROR_FILE = os.path.join(LOG_DIR, "error.log")


def _write(filepath, source, message, detail=""):
    os.makedirs(LOG_DIR, exist_ok=True)
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    with open(filepath, "a") as f:
        f.write(f"[{timestamp}] [{source}] {message}\n")
        if detail:
            f.write(f"  {detail}\n")


def log_info(source, message, detail=""):
    _write(INFO_FILE, source, message, detail)


def log_error(source, message, detail=""):
    _write(ERROR_FILE, source, message, detail)


def log_exception(source):
    log_error(source, "Exception", traceback.format_exc())
