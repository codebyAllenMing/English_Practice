import { useState, useEffect, useRef } from 'react'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

// 啟動時向 GitHub Releases 檢查新版(latest.json);提示與完成為非阻塞角落卡片,
// 下載安裝期間升級成全螢幕 modal 鎖住整個 app(檔案正在原地替換,不能讓操作混進來)
function UpdateBanner() {
    const [phase, setPhase] = useState(null) // null = 無事 | 'available' | 'downloading' | 'ready' | 'error'
    const [version, setVersion] = useState('')
    const [pct, setPct] = useState(0)
    const [error, setError] = useState('')
    const updateRef = useRef(null)

    useEffect(() => {
        if (!import.meta.env.PROD) return // dev 模式沒有簽章產物,不檢查
        check().then((update) => {
            if (update) {
                updateRef.current = update
                setVersion(update.version)
                setPhase('available')
            }
        }).catch(() => {}) // 離線或 API 失敗時安靜略過,下次啟動再試
    }, [])

    // 下載中把鍵盤也鎖掉:遮罩擋得住點擊,但練習頁快捷鍵掛在 window 上,要用 capture 搶先吞掉
    useEffect(() => {
        if (phase !== 'downloading') return
        const block = (e) => {
            e.stopPropagation()
            e.preventDefault()
        }
        window.addEventListener('keydown', block, true)
        return () => window.removeEventListener('keydown', block, true)
    }, [phase])

    const handleUpdate = async () => {
        setPhase('downloading')
        setPct(0)
        try {
            let total = 0
            let received = 0
            await updateRef.current.downloadAndInstall((e) => {
                if (e.event === 'Started') total = e.data.contentLength || 0
                if (e.event === 'Progress' && total > 0) {
                    received += e.data.chunkLength
                    setPct(Math.min(100, Math.round(received / total * 100)))
                }
            })
            setPhase('ready')
        } catch (err) {
            setError(String(err))
            setPhase('error')
        }
    }

    if (!phase) return null

    if (phase === 'downloading') {
        return (
            <div className="fixed inset-0 z-50 bg-black/50 flex items-center justify-center">
                <div className="w-[320px] bg-surface border border-edge rounded-xl shadow-xl p-5">
                    <p className="text-sm text-ink-soft mb-2">下載更新中... {pct}%</p>
                    <div className="w-full bg-muted rounded-full h-2">
                        <div className="bg-primary h-2 rounded-full transition-all duration-300" style={{ width: `${pct}%` }} />
                    </div>
                    <p className="text-xs text-ink-faint mt-3">更新安裝中,請勿關閉視窗。</p>
                </div>
            </div>
        )
    }

    return (
        <div className="fixed bottom-4 right-4 z-50 w-[320px] bg-surface border border-edge rounded-xl shadow-lg p-4">
            {phase === 'available' && (
                <>
                    <p className="text-sm font-bold mb-1">有新版本 v{version}</p>
                    <p className="text-xs text-ink-faint mb-3">更新會在背景下載,完成後重啟即生效。</p>
                    <div className="flex justify-end gap-2">
                        <button
                            className="px-3 py-1.5 text-xs text-ink-soft border border-edge-strong rounded-lg hover:bg-card"
                            onClick={() => setPhase(null)}
                        >
                            稍後
                        </button>
                        <button
                            className="px-3 py-1.5 bg-primary text-white text-xs rounded-lg hover:bg-primary-hover"
                            onClick={handleUpdate}
                        >
                            立即更新
                        </button>
                    </div>
                </>
            )}

            {phase === 'ready' && (
                <>
                    <p className="text-sm font-bold mb-1">更新完成</p>
                    <p className="text-xs text-ink-faint mb-3">重新啟動後套用 v{version}。</p>
                    <div className="flex justify-end gap-2">
                        <button
                            className="px-3 py-1.5 text-xs text-ink-soft border border-edge-strong rounded-lg hover:bg-card"
                            onClick={() => setPhase(null)}
                        >
                            下次啟動時套用
                        </button>
                        <button
                            className="px-3 py-1.5 bg-primary text-white text-xs rounded-lg hover:bg-primary-hover"
                            onClick={() => relaunch()}
                        >
                            立即重啟
                        </button>
                    </div>
                </>
            )}

            {phase === 'error' && (
                <>
                    <p className="text-sm text-red-500 dark:text-red-400 mb-3 break-all">更新失敗:{error}</p>
                    <div className="flex justify-end">
                        <button
                            className="px-3 py-1.5 text-xs text-ink-soft border border-edge-strong rounded-lg hover:bg-card"
                            onClick={() => setPhase(null)}
                        >
                            關閉
                        </button>
                    </div>
                </>
            )}
        </div>
    )
}

export default UpdateBanner
