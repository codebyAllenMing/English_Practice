#!/bin/zsh
# 發佈新版:帶 updater 簽章 build → 整理產物(檔名去空白)→ 產 latest.json → 印出 gh release 指令
# 版本號以 app/src-tauri/tauri.conf.json 的 version 為準;私鑰在 ~/.tauri/(不進版)
set -euo pipefail

REPO="codebyAllenMing/English_Practice"
ROOT="$(cd "$(dirname "$0")" && pwd)"
KEY="$HOME/.tauri/english-practice.key"

[ -f "$KEY" ] || { echo "找不到簽章私鑰:$KEY"; exit 1; }

VERSION=$(node -p "require('$ROOT/app/src-tauri/tauri.conf.json').version")
echo "==> 發佈 v$VERSION"

# bundler 只認 TAURI_SIGNING_PRIVATE_KEY(路徑或內容皆可),_PATH 後綴無效
export TAURI_SIGNING_PRIVATE_KEY="$KEY"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""

cd "$ROOT/app"
npm run tauri build

BUNDLE="$ROOT/app/src-tauri/target/release/bundle"
OUT="$ROOT/release-out/v$VERSION"
rm -rf "$OUT" && mkdir -p "$OUT"

# app 名為中文(語言影子練習),GitHub 資產一律轉 ASCII 底線命名(空白/非 ASCII 會被改寫)
cp "$BUNDLE/macos/語言影子練習.app.tar.gz"     "$OUT/Language_Shadowing_${VERSION}_aarch64.app.tar.gz"
cp "$BUNDLE/macos/語言影子練習.app.tar.gz.sig" "$OUT/Language_Shadowing_${VERSION}_aarch64.app.tar.gz.sig"
cp "$BUNDLE/dmg/語言影子練習_${VERSION}_aarch64.dmg" "$OUT/Language_Shadowing_${VERSION}_aarch64.dmg"

SIG=$(cat "$OUT/Language_Shadowing_${VERSION}_aarch64.app.tar.gz.sig")
PUB_DATE=$(date -u "+%Y-%m-%dT%H:%M:%SZ")

cat > "$OUT/latest.json" <<EOF
{
	"version": "$VERSION",
	"notes": "見 https://github.com/$REPO/releases/tag/v$VERSION",
	"pub_date": "$PUB_DATE",
	"platforms": {
		"darwin-aarch64": {
			"signature": "$SIG",
			"url": "https://github.com/$REPO/releases/download/v$VERSION/Language_Shadowing_${VERSION}_aarch64.app.tar.gz"
		}
	}
}
EOF

echo ""
echo "==> 產物已就緒:$OUT"
ls -lh "$OUT"
echo ""
echo "==> 建立 GitHub Release(自己執行):"
echo "gh release create v$VERSION \\"
echo "  \"$OUT/Language_Shadowing_${VERSION}_aarch64.dmg\" \\"
echo "  \"$OUT/Language_Shadowing_${VERSION}_aarch64.app.tar.gz\" \\"
echo "  \"$OUT/Language_Shadowing_${VERSION}_aarch64.app.tar.gz.sig\" \\"
echo "  \"$OUT/latest.json\" \\"
echo "  --title \"v$VERSION\" --notes \"版本說明\""
