import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import ConfirmDeleteDialog from '../Components/ConfirmDeleteDialog'
import { useCorrection } from '../Hooks/useCorrection'

function Download() {
    const [url, setUrl] = useState('')
    const [status, setStatus] = useState('')
    const [progress, setProgress] = useState('')
    const [title, setTitle] = useState('')
    const [downloading, setDownloading] = useState(false)
    const [downloads, setDownloads] = useState([])
    const [pendingTitle, setPendingTitle] = useState('')
    const [folderName, setFolderName] = useState('')
    const [fetching, setFetching] = useState(false)
    const [showPending, setShowPending] = useState(true)
    const [showDone, setShowDone] = useState(false)
    const [canCorrect, setCanCorrect] = useState(false)
    const [confirmDelete, setConfirmDelete] = useState('')
    const [tools, setTools] = useState(null) // {ytDlp, ffmpeg}
    // 校正狀態在全域 Provider,切換分頁不會遺失;correctStatus 改用 provider 的 lastMessage
    const { correcting, lastMessage: correctStatus, startCorrect } = useCorrection()

    const loadDownloads = async () => {
        try {
            const list = await invoke('list_podcasts')
            setDownloads(list)
        } catch (err) {
            console.error(err)
        }
    }

    // CLI 模式不需要 key;API 模式要有 key 才能按
    const loadConfig = () => {
        invoke('get_config')
            .then((config) => setCanCorrect((config.correction_mode || 'api') === 'cli' || !!config.anthropic_api_key))
            .catch(console.error)
    }

    useEffect(() => {
        loadDownloads()
        loadConfig()
        invoke('tools_status').then(setTools).catch(console.error)
        // 設定存檔後同步按鈕狀態;校正完成後刷新清單(含在別的分頁跑完的情況)
        window.addEventListener('config-saved', loadConfig)
        window.addEventListener('correction-done', loadDownloads)

        const unlistenProgress = listen('download-progress', (e) => {
            if (e.payload === 'converting') {
                setProgress('轉換 MP3 中...')
            } else {
                setProgress(`${e.payload}%`)
            }
        })
        const unlistenTitle = listen('download-title', (e) => {
            setTitle(e.payload)
        })

        return () => {
            window.removeEventListener('config-saved', loadConfig)
            window.removeEventListener('correction-done', loadDownloads)
            unlistenProgress.then(fn => fn())
            unlistenTitle.then(fn => fn())
        }
    }, [])

    const handleFetch = async () => {
        if (!url.trim()) return
        setFetching(true)
        setStatus('')
        try {
            const info = await invoke('fetch_title', { url: url.trim() })
            setPendingTitle(info.title)
            setFolderName(info.folder)
        } catch (err) {
            setStatus(`錯誤：${err}`)
        } finally {
            setFetching(false)
        }
    }

    const handleConfirmDownload = async () => {
        if (!folderName.trim()) return
        setDownloading(true)
        setStatus('')
        setProgress('準備中...')
        setTitle('')
        try {
            const result = await invoke('download_audio', { url: url.trim(), folder: folderName.trim() })
            setStatus(`下載完成：${result}`)
            setUrl('')
            setPendingTitle('')
            setFolderName('')
            loadDownloads()
        } catch (err) {
            setStatus(`錯誤：${err}`)
        } finally {
            setDownloading(false)
            setProgress('')
            setTitle('')
        }
    }

    const handleCancel = () => {
        setPendingTitle('')
        setFolderName('')
    }

    const handleDelete = async (name) => {
        setConfirmDelete('')
        try {
            await invoke('delete_podcast', { folder: name })
            loadDownloads()
        } catch (err) {
            setStatus(`錯誤：${err}`)
        }
    }

    const renderDelete = (name) => (
        <button
            className="text-ink-faint/60 hover:text-red-500 dark:hover:text-red-400 shrink-0 disabled:opacity-50"
            onClick={() => setConfirmDelete(name)}
            disabled={correcting === name || downloading}
            title="刪除"
        >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth="2">
                <path strokeLinecap="round" strokeLinejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0" />
            </svg>
        </button>
    )

    const missingTools = tools ? [!tools.ytDlp && 'yt-dlp', !tools.ffmpeg && 'ffmpeg'].filter(Boolean) : []

    return (
        <div>
            <h1 className="text-2xl font-bold mb-6">下載 Podcast</h1>

            {missingTools.length > 0 && (
                <div className="mb-4 p-4 bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-900/60 rounded-lg text-sm text-amber-800 dark:text-amber-200">
                    找不到必要工具:{missingTools.join('、')}。請在終端機執行
                    <code className="mx-1 px-1.5 py-0.5 bg-amber-100 dark:bg-amber-900/60 rounded">brew install {missingTools.join(' ')}</code>
                    後重新啟動 app。
                </div>
            )}
            <div className="flex gap-3 mb-4">
                <input
                    type="text"
                    className="flex-1 px-4 py-2 border border-edge-strong rounded-lg outline-none focus:border-ink-faint"
                    placeholder="貼上 YouTube 連結..."
                    value={url}
                    onChange={(e) => setUrl(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && handleFetch()}
                    disabled={fetching || downloading || !!pendingTitle}
                />
                <button
                    className="px-6 py-2 bg-primary text-white rounded-lg disabled:opacity-50 disabled:cursor-not-allowed hover:bg-primary-hover"
                    onClick={handleFetch}
                    disabled={fetching || downloading || !url.trim() || !!pendingTitle}
                >
                    {fetching ? '取得中...' : '取得標題'}
                </button>
            </div>

            {pendingTitle && !downloading && (
                <div className="mb-4 p-4 bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-900/60 rounded-lg">
                    <p className="text-sm text-amber-800 dark:text-amber-200 mb-2">原始標題：{pendingTitle}</p>
                    <label className="block text-sm font-medium text-ink-soft mb-2">資料夾名稱（可修改）</label>
                    <input
                        type="text"
                        className="w-full px-4 py-2 border border-edge-strong rounded-lg outline-none focus:border-ink-faint text-sm mb-3"
                        value={folderName}
                        onChange={(e) => setFolderName(e.target.value)}
                    />
                    <div className="flex gap-2">
                        <button
                            className="px-4 py-2 bg-primary text-white text-sm rounded-lg hover:bg-primary-hover disabled:opacity-50"
                            onClick={handleConfirmDownload}
                            disabled={!folderName.trim()}
                        >
                            確認下載
                        </button>
                        <button
                            className="px-4 py-2 text-sm text-ink-soft border border-edge-strong rounded-lg hover:bg-card"
                            onClick={handleCancel}
                        >
                            取消
                        </button>
                    </div>
                </div>
            )}

            {downloading && (
                <div className="mb-4 p-4 bg-blue-50 dark:bg-blue-950/40 border border-blue-200 dark:border-blue-900/60 rounded-lg">
                    {title && <p className="text-sm text-blue-800 dark:text-blue-300 font-medium mb-2">{title}</p>}
                    <div className="flex items-center gap-3">
                        <svg className="animate-spin h-4 w-4 text-blue-500 dark:text-blue-400" viewBox="0 0 24 24" fill="none">
                            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                        </svg>
                        <span className="text-sm text-blue-600 dark:text-blue-400">{progress}</span>
                    </div>
                    {progress && progress !== '準備中...' && progress !== '轉換 MP3 中...' && (
                        <div className="mt-2 w-full bg-blue-100 dark:bg-blue-900/50 rounded-full h-2">
                            <div
                                className="bg-blue-500 h-2 rounded-full transition-all duration-300"
                                style={{ width: progress }}
                            />
                        </div>
                    )}
                </div>
            )}

            {!downloading && status && (
                <p className={`text-sm mb-6 ${status.startsWith('錯誤') ? 'text-red-500 dark:text-red-400' : 'text-green-600 dark:text-green-400'}`}>
                    {status}
                </p>
            )}

            {downloads.filter(d => !d.transcribed).length > 0 && (
                <div className="mb-6">
                    <button
                        className="flex items-center gap-2 text-lg font-semibold mb-3 hover:text-ink-soft"
                        onClick={() => setShowPending(!showPending)}
                    >
                        <span className={`inline-block transition-transform ${showPending ? 'rotate-90' : ''}`}>▶</span>
                        未轉譯 ({downloads.filter(d => !d.transcribed).length})
                    </button>
                    {showPending && (
                        <ul className="space-y-2">
                            {downloads.filter(d => !d.transcribed).map((d) => (
                                <li key={d.name} className="px-4 py-3 bg-amber-50 dark:bg-amber-950/40 rounded-lg border border-amber-200 dark:border-amber-900/60 text-sm flex items-center justify-between gap-3">
                                    <span className="flex-1">{d.name}</span>
                                    <span className="text-xs text-amber-600 dark:text-amber-400 shrink-0">待轉譯</span>
                                    {renderDelete(d.name)}
                                </li>
                            ))}
                        </ul>
                    )}
                </div>
            )}

            {downloads.filter(d => d.transcribed).length > 0 && (
                <div>
                    <button
                        className="flex items-center gap-2 text-lg font-semibold mb-3 hover:text-ink-soft"
                        onClick={() => setShowDone(!showDone)}
                    >
                        <span className={`inline-block transition-transform ${showDone ? 'rotate-90' : ''}`}>▶</span>
                        已轉譯 ({downloads.filter(d => d.transcribed).length})
                    </button>
                    {correctStatus && (
                        <p className={`text-sm mb-3 ${correctStatus.startsWith('錯誤') ? 'text-red-500 dark:text-red-400' : 'text-green-600 dark:text-green-400'}`}>
                            {correctStatus}
                        </p>
                    )}
                    {showDone && (
                        <ul className="space-y-2">
                            {downloads.filter(d => d.transcribed).map((d) => (
                                <li key={d.name} className="px-4 py-3 bg-green-50 dark:bg-green-950/40 rounded-lg border border-green-200 dark:border-green-900/60 text-sm flex items-center justify-between gap-3">
                                    <span className="flex-1">{d.name}</span>
                                    {correcting === d.name ? (
                                        <span className="flex items-center gap-2 text-xs text-blue-600 dark:text-blue-400 shrink-0">
                                            <svg className="animate-spin h-3 w-3" viewBox="0 0 24 24" fill="none">
                                                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                                                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                                            </svg>
                                            校正中...
                                        </span>
                                    ) : d.corrected ? (
                                        <span className="flex items-center gap-2 shrink-0">
                                            <span className="text-xs text-purple-600 dark:text-purple-400">✨ 已校正</span>
                                            <button
                                                className="text-xs text-ink-faint hover:text-ink-soft underline disabled:opacity-50"
                                                onClick={() => startCorrect(d.name)}
                                                disabled={!!correcting || !canCorrect}
                                            >
                                                重新校正
                                            </button>
                                        </span>
                                    ) : (
                                        <button
                                            className="px-3 py-1 text-xs bg-purple-600 text-white rounded-md hover:bg-purple-500 disabled:opacity-50 disabled:cursor-not-allowed shrink-0"
                                            onClick={() => startCorrect(d.name)}
                                            disabled={!!correcting || !canCorrect}
                                            title={canCorrect ? '用 AI 校正說話者名字與錯字' : '請先在設定填入 Anthropic API Key'}
                                        >
                                            AI 校正
                                        </button>
                                    )}
                                    {renderDelete(d.name)}
                                </li>
                            ))}
                        </ul>
                    )}
                </div>
            )}

            {downloads.length === 0 && (
                <p className="text-ink-faint text-sm">尚無下載</p>
            )}

            <ConfirmDeleteDialog name={confirmDelete} onCancel={() => setConfirmDelete('')} onConfirm={handleDelete} />
        </div>
    )
}

export default Download
