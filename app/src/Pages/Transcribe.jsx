import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useTranscribe } from '../Hooks/useTranscribe'

function Transcribe() {
    const [folders, setFolders] = useState([])
    const [selected, setSelected] = useState('')
    const { processing, progress, folder, result, error, startTranscribe, clearResult } = useTranscribe()

    const loadFolders = async () => {
        try {
            const list = await invoke('list_untranscribed')
            setFolders(list)
        } catch (err) {
            console.error(err)
        }
    }

    useEffect(() => {
        // eslint-disable-next-line react-hooks/set-state-in-effect
        loadFolders()
    }, [])

    useEffect(() => {
        // eslint-disable-next-line react-hooks/set-state-in-effect
        if (result) loadFolders()
    }, [result])

    const handleTranscribe = async () => {
        if (!selected) return
        clearResult()
        startTranscribe(selected)
        setSelected('')
    }

    return (
        <div>
            <h1 className="text-2xl font-bold mb-6">轉譯</h1>

            {processing && (
                <div className="mb-4 p-4 bg-blue-50 dark:bg-blue-950/40 border border-blue-200 dark:border-blue-900/60 rounded-lg">
                    <p className="text-sm text-blue-800 dark:text-blue-300 font-medium mb-2">正在轉譯：{folder}</p>
                    <div className="flex items-center gap-3">
                        <svg className="animate-spin h-4 w-4 text-blue-500 dark:text-blue-400" viewBox="0 0 24 24" fill="none">
                            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                        </svg>
                        <span className="text-sm text-blue-600 dark:text-blue-400">{progress}</span>
                    </div>
                </div>
            )}

            {result && (
                <p className="text-sm mb-4 text-green-600 dark:text-green-400">轉譯完成：{result}</p>
            )}
            {error && (
                <p className="text-sm mb-4 text-red-500 dark:text-red-400">錯誤：{error}</p>
            )}

            {folders.length === 0 && !processing ? (
                <p className="text-ink-faint text-sm">沒有需要轉譯的 podcast</p>
            ) : (
                <div className="flex gap-3 mb-4">
                    <select
                        className="flex-1 px-4 py-2 border border-edge-strong rounded-lg outline-none text-sm"
                        value={selected}
                        onChange={(e) => setSelected(e.target.value)}
                        disabled={processing}
                    >
                        <option value="">選擇 podcast...</option>
                        {folders.map((name) => (
                            <option key={name} value={name}>{name}</option>
                        ))}
                    </select>
                    <button
                        className="px-6 py-2 bg-primary text-white rounded-lg disabled:opacity-50 disabled:cursor-not-allowed hover:bg-primary-hover text-sm"
                        onClick={handleTranscribe}
                        disabled={processing || !selected}
                    >
                        {processing ? '轉譯中...' : '開始轉譯'}
                    </button>
                </div>
            )}
        </div>
    )
}

export default Transcribe
