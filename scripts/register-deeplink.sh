#!/usr/bin/env bash
# 注册 nsswitch:// 深链协议到系统。
#
# macOS: 用 osascript 调用系统注册（适用于开发/打包后的 .app）。
# Linux: 写 ~/.local/share/applications/ns-switch.desktop + update-desktop-database。
# Windows: 需注册表（见脚本注释），本脚本在 Windows 下提示手动操作。
set -euo pipefail

URL_SCHEME="nsswitch"
BIN="$(cd "$(dirname "$0")/.." && pwd)/target/debug/ns-switch"

case "$(uname -s)" in
  Darwin)
    # 若存在 .app 包，用其路径；否则用源码二进制（仅开发期生效）。
    APP="${NS_SWITCH_APP:-}"
    if [[ -z "$APP" ]]; then
      APP="/Applications/ns-switch.app"
    fi
    if [[ -d "$APP" ]]; then
      osascript -e 'on run argv
        set appPath to item 1 of argv
        set bundleId to "com.ns-switch.app"
        do shell script "/usr/libexec/PlistBuddy -c \"Add :CFBundleURLTypes array\" '"$APP"'/Contents/Info.plist 2>/dev/null || true"
        do shell script "/usr/libexec/PlistBuddy -c \"Add :CFBundleURLTypes:0 dict\" '"$APP"'/Contents/Info.plist 2>/dev/null || true"
        do shell script "/usr/libexec/PlistBuddy -c \"Add :CFBundleURLTypes:0:CFBundleURLName string com.ns-switch.app\" '"$APP"'/Contents/Info.plist 2>/dev/null || true"
        do shell script "/usr/libexec/PlistBuddy -c \"Add :CFBundleURLTypes:0:CFBundleURLSchemes array\" '"$APP"'/Contents/Info.plist 2>/dev/null || true"
        do shell script "/usr/libexec/PlistBuddy -c \"Add :CFBundleURLTypes:0:CFBundleURLSchemes:0 string '"$URL_SCHEME"'\" '"$APP"'/Contents/Info.plist 2>/dev/null || true"
      end run' "$APP"
      echo "已注册 nsswitch:// 到 $APP（重新打开应用生效）。"
    else
      echo "未找到 .app（$APP）。开发期可直接: open 'nsswitch://import-provider?name=test'"
      echo "提示: 打包时在 Info.plist 加 CFBundleURLTypes(CFBundleURLSchemes=[nsswitch])。"
    fi
    ;;
  Linux)
    local_app="$HOME/.local/share/applications"
    mkdir -p "$local_app"
    cat > "$local_app/ns-switch.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=ns-switch
Comment=AI provider switch
Exec=$BIN %u
Terminal=false
MimeType=x-scheme-handler/$URL_SCHEME;
Categories=Development;
EOF
    chmod +x "$local_app/ns-switch.desktop"
    update-desktop-database "$local_app" 2>/dev/null || xdg-mime default ns-switch.desktop "x-scheme-handler/$URL_SCHEME" 2>/dev/null || true
    echo "已注册 nsswitch:// 到 $local_app/ns-switch.desktop。"
    ;;
  *)
    echo "Windows: 需注册表。管理员 PowerShell:"
    echo '  New-Item "HKCU:\Software\Classes\nsswitch" -Force'
    echo '  Set-Item "HKCU:\Software\Classes\nsswitch" "(Default)" "URL:nsswitch Protocol"'
    echo '  Set-Item "HKCU:\Software\Classes\nsswitch\shell\open\command" "(Default)" "\"C:\\path\\ns-switch.exe\" \"%1\""'
    ;;
esac
