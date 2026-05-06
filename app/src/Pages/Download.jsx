import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

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

    const loadDownloads = async () => {
        try {
            const list = await invoke('list_podcasts')
            setDownloads(list)
        } catch (err) {
            console.error(err)
        }
    }

    useEffect(() => {
        loadDownloads()

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

    return (
        <div>
            <h1 className="text-2xl font-bold mb-6">下載 Podcast</h1>
            <div className="flex gap-3 mb-4">
                <input
                    type="text"
                    className="flex-1 px-4 py-2 border border-gray-300 rounded-lg outline-none focus:border-gray-500"
                    placeholder="貼上 YouTube 連結..."
                    value={url}
                    onChange={(e) => setUrl(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && handleFetch()}
                    disabled={fetching || downloading || !!pendingTitle}
                />
                <button
                    className="px-6 py-2 bg-gray-800 text-white rounded-lg disabled:opacity-50 disabled:cursor-not-allowed hover:bg-gray-700"
                    onClick={handleFetch}
                    disabled={fetching || downloading || !url.trim() || !!pendingTitle}
                >
                    {fetching ? '取得中...' : '取得標題'}
                </button>
            </div>

            {pendingTitle && !downloading && (
                <div className="mb-4 p-4 bg-amber-50 border border-amber-200 rounded-lg">
                    <p className="text-sm text-amber-800 mb-2">原始標題：{pendingTitle}</p>
                    <label className="block text-sm font-medium text-gray-700 mb-2">資料夾名稱（可修改）</label>
                    <input
                        type="text"
                        className="w-full px-4 py-2 border border-gray-300 rounded-lg outline-none focus:border-gray-500 text-sm mb-3"
                        value={folderName}
                        onChange={(e) => setFolderName(e.target.value)}
                    />
                    <div className="flex gap-2">
                        <button
                            className="px-4 py-2 bg-gray-800 text-white text-sm rounded-lg hover:bg-gray-700 disabled:opacity-50"
                            onClick={handleConfirmDownload}
                            disabled={!folderName.trim()}
                        >
                            確認下載
                        </button>
                        <button
                            className="px-4 py-2 text-sm text-gray-600 border border-gray-300 rounded-lg hover:bg-gray-50"
                            onClick={handleCancel}
                        >
                            取消
                        </button>
                    </div>
                </div>
            )}

            {downloading && (
                <div className="mb-4 p-4 bg-blue-50 border border-blue-200 rounded-lg">
                    {title && <p className="text-sm text-blue-800 font-medium mb-2">{title}</p>}
                    <div className="flex items-center gap-3">
                        <svg className="animate-spin h-4 w-4 text-blue-500" viewBox="0 0 24 24" fill="none">
                            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                        </svg>
                        <span className="text-sm text-blue-600">{progress}</span>
                    </div>
                    {progress && progress !== '準備中...' && progress !== '轉換 MP3 中...' && (
                        <div className="mt-2 w-full bg-blue-100 rounded-full h-2">
                            <div
                                className="bg-blue-500 h-2 rounded-full transition-all duration-300"
                                style={{ width: progress }}
                            />
                        </div>
                    )}
                </div>
            )}

            {!downloading && status && (
                <p className={`text-sm mb-6 ${status.startsWith('錯誤') ? 'text-red-500' : 'text-green-600'}`}>
                    {status}
                </p>
            )}

            {downloads.filter(d => !d.transcribed).length > 0 && (
                <div className="mb-6">
                    <button
                        className="flex items-center gap-2 text-lg font-semibold mb-3 hover:text-gray-600"
                        onClick={() => setShowPending(!showPending)}
                    >
                        <span className={`inline-block transition-transform ${showPending ? 'rotate-90' : ''}`}>▶</span>
                        未轉譯 ({downloads.filter(d => !d.transcribed).length})
                    </button>
                    {showPending && (
                        <ul className="space-y-2">
                            {downloads.filter(d => !d.transcribed).map((d) => (
                                <li key={d.name} className="px-4 py-3 bg-amber-50 rounded-lg border border-amber-200 text-sm flex items-center justify-between">
                                    <span>{d.name}</span>
                                    <span className="text-xs text-amber-600">待轉譯</span>
                                </li>
                            ))}
                        </ul>
                    )}
                </div>
            )}

            {downloads.filter(d => d.transcribed).length > 0 && (
                <div>
                    <button
                        className="flex items-center gap-2 text-lg font-semibold mb-3 hover:text-gray-600"
                        onClick={() => setShowDone(!showDone)}
                    >
                        <span className={`inline-block transition-transform ${showDone ? 'rotate-90' : ''}`}>▶</span>
                        已轉譯 ({downloads.filter(d => d.transcribed).length})
                    </button>
                    {showDone && (
                        <ul className="space-y-2">
                            {downloads.filter(d => d.transcribed).map((d) => (
                                <li key={d.name} className="px-4 py-3 bg-green-50 rounded-lg border border-green-200 text-sm flex items-center justify-between">
                                    <span>{d.name}</span>
                                    <span className="text-xs text-green-600">已完成</span>
                                </li>
                            ))}
                        </ul>
                    )}
                </div>
            )}

            {downloads.length === 0 && (
                <p className="text-gray-400 text-sm">尚無下載</p>
            )}
        </div>
    )
}

export default Download
