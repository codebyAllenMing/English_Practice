import os
import traceback
from datetime import datetime

LOG_DIR = os.path.join(os.path.dirname(__file__), "logs")
LOG_FILE = os.path.join(LOG_DIR, "error.log")


def log_error(source, message, detail=""):
    os.makedirs(LOG_DIR, exist_ok=True)
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    with open(LOG_FILE, "a") as f:
        f.write(f"[{timestamp}] [{source}] {message}\n")
        if detail:
            f.write(f"  {detail}\n")
        f.write("\n")


def log_exception(source):
    log_error(source, "Exception", traceback.format_exc())
