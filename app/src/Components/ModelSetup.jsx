import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

// 啟動時檢查模型是否齊全;缺件時顯示下載引導(首次啟動的初始化流程)
function ModelSetup() {
    const [status, setStatus] = useState(null) // null = 檢查中 | {ready, missing, totalMb}
    const [dismissed, setDismissed] = useState(false)
    const [downloading, setDownloading] = useState(false)
    const [progress, setProgress] = useState(null) // {name, received, total, index, count}
    const [error, setError] = useState('')

    useEffect(() => {
        invoke('models_status').then(setStatus).catch(console.error)
        const unlisten = listen('model-progress', (e) => setProgress(e.payload))
        return () => { unlisten.then(fn => fn()) }
    }, [])

    if (!status || status.ready || dismissed) return null

    const handleDownload = async () => {
        setDownloading(true)
        setError('')
        try {
            await invoke('download_models')
            const s = await invoke('models_status')
            setStatus(s)
        } catch (err) {
            setError(String(err))
        } finally {
            setDownloading(false)
            setProgress(null)
        }
    }

    const pct = progress && progress.total > 0 ? Math.min(100, Math.round(progress.received / progress.total * 100)) : 0

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
            <div className="bg-surface border border-edge rounded-xl shadow-lg w-[460px] p-6">
                <h2 className="text-lg font-bold mb-2">初始化:下載模型</h2>
                <p className="text-sm text-ink-soft mb-4">
                    轉譯與練習功能需要以下模型(共約 {status.totalMb} MB),下載一次即可離線使用。
                </p>

                <ul className="space-y-1 mb-4">
                    {status.missing.map((m) => (
                        <li key={m.name} className="flex justify-between text-sm">
                            <span>{m.name}</span>
                            <span className="text-ink-faint">{m.mb} MB</span>
                        </li>
                    ))}
                </ul>

                {downloading && (
                    <div className="mb-4">
                        <p className="text-sm text-blue-600 dark:text-blue-400 mb-2">
                            {progress
                                ? `(${progress.index + 1}/${progress.count})${progress.name} — ${Math.round(progress.received / 1048576)} MB`
                                : '連線中...'}
                        </p>
                        <div className="w-full bg-muted rounded-full h-2">
                            <div className="bg-primary h-2 rounded-full transition-all duration-300" style={{ width: `${pct}%` }} />
                        </div>
                    </div>
                )}

                {error && <p className="text-sm text-red-500 dark:text-red-400 mb-4">{error}</p>}

                <div className="flex items-center justify-end gap-3">
                    <button
                        className="px-4 py-2 text-sm text-ink-soft border border-edge-strong rounded-lg hover:bg-card disabled:opacity-50"
                        onClick={() => setDismissed(true)}
                        disabled={downloading}
                    >
                        稍後
                    </button>
                    <button
                        className="px-4 py-2 bg-primary text-white text-sm rounded-lg hover:bg-primary-hover disabled:opacity-50"
                        onClick={handleDownload}
                        disabled={downloading}
                    >
                        {downloading ? '下載中...' : '開始下載'}
                    </button>
                </div>
            </div>
        </div>
    )
}

export default ModelSetup
