import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import Settings from '../Components/Settings'
import ConfirmDeleteDialog from '../Components/ConfirmDeleteDialog'
import { useCorrection } from '../Hooks/useCorrection'

function Correct() {
    const [podcasts, setPodcasts] = useState([])
    const [showSettings, setShowSettings] = useState(false)
    const [hasApiKey, setHasApiKey] = useState(false)
    const [mode, setMode] = useState('api')
    const [confirmDelete, setConfirmDelete] = useState('')
    // 校正狀態在全域 Provider,切換分頁不會遺失
    const { correcting, results, startCorrect, reportResult, clearResult } = useCorrection()

    // CLI 模式不需要 key;API 模式要有 key 才能按
    const canCorrect = mode === 'cli' || hasApiKey

    const load = async () => {
        try {
            const list = await invoke('list_podcasts')
            setPodcasts(list.filter((d) => d.transcribed))
        } catch (err) {
            console.error(err)
        }
    }

    const loadConfig = () => {
        invoke('get_config')
            .then((config) => {
                setHasApiKey(!!config.anthropic_api_key)
                setMode(config.correction_mode || 'api')
            })
            .catch(console.error)
    }

    useEffect(() => {
        // eslint-disable-next-line react-hooks/set-state-in-effect
        load()
        loadConfig()
        // 設定存檔後同步模式徽章與按鈕狀態;校正完成後刷新清單(含在別的分頁跑完回來的情況)
        window.addEventListener('config-saved', loadConfig)
        window.addEventListener('correction-done', load)
        return () => {
            window.removeEventListener('config-saved', loadConfig)
            window.removeEventListener('correction-done', load)
        }
    }, [])

    const handleDelete = async (name) => {
        setConfirmDelete('')
        try {
            await invoke('delete_podcast', { folder: name })
            clearResult(name)
            load()
        } catch (err) {
            reportResult(name, { error: String(err) })
        }
    }

    const renderDelete = (d) => (
        <button
            className="text-ink-faint/60 hover:text-red-500 dark:hover:text-red-400 shrink-0 disabled:opacity-50"
            onClick={() => setConfirmDelete(d.name)}
            disabled={correcting === d.name}
            title="刪除"
        >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth="2">
                <path strokeLinecap="round" strokeLinejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0" />
            </svg>
        </button>
    )


    const renderAction = (d) => {
        if (correcting === d.name) {
            return (
                <span className="flex items-center gap-2 text-xs text-blue-600 dark:text-blue-400 shrink-0">
                    <svg className="animate-spin h-3 w-3" viewBox="0 0 24 24" fill="none">
                        <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                        <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                    </svg>
                    校正中...
                </span>
            )
        }
        if (d.corrected) {
            return (
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
            )
        }
        return (
            <button
                className="px-3 py-1 text-xs bg-purple-600 text-white rounded-md hover:bg-purple-500 disabled:opacity-50 disabled:cursor-not-allowed shrink-0"
                onClick={() => startCorrect(d.name)}
                disabled={!!correcting || !canCorrect}
                title={canCorrect ? '用 AI 校正說話者名字與錯字' : '請先在設定填入 Anthropic API Key'}
            >
                AI 校正
            </button>
        )
    }

    const renderResult = (d) => {
        const res = results[d.name]
        if (!res) return null
        if (res.error) {
            return <p className="text-xs text-red-500 dark:text-red-400 mt-2">{res.error}</p>
        }
        return (
            <p className="text-xs text-green-600 dark:text-green-400 mt-2">
                校正完成:說話者 {res.speakers} 位、修正 {res.fixes} 行
                {res.skipped > 0 && `(${res.skipped} 行行號無效已略過)`}
            </p>
        )
    }

    const pending = podcasts.filter((d) => !d.corrected)
    const done = podcasts.filter((d) => d.corrected)

    return (
        <div>
            <div className="flex items-center justify-between mb-2">
                <h1 className="text-2xl font-bold">AI 校正</h1>
                <button
                    className="flex items-center gap-1.5 px-3 py-1.5 text-sm text-ink-soft border border-edge-strong rounded-lg hover:bg-card"
                    onClick={() => setShowSettings(true)}
                >
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth="2">
                        <path strokeLinecap="round" strokeLinejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                        <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                    </svg>
                    設定
                </button>
            </div>
            {showSettings && <Settings section="correct" onClose={() => setShowSettings(false)} />}
            <p className="text-sm text-ink-faint mb-3">
                用 AI 修正說話者名字與明顯的轉譯錯字(只修錯字與標點,不改寫句子,保留原始口語)。
            </p>

            <div className="mb-6 flex items-center gap-2 text-sm">
                <span className="text-ink-faint">目前模式:</span>
                <span className={`px-2 py-0.5 rounded-full text-xs font-medium ${mode === 'cli' ? 'bg-blue-100 dark:bg-blue-900/50 text-blue-700 dark:text-blue-300' : 'bg-purple-100 dark:bg-purple-900/50 text-purple-700 dark:text-purple-300'}`}>
                    {mode === 'cli' ? '🖥 本機 Claude CLI(訂閱額度)' : '☁️ Anthropic API(Key 計費)'}
                </span>
                <span className="text-xs text-ink-faint">可用右側「設定」切換</span>
            </div>

            {mode === 'api' && !hasApiKey && (
                <div className="mb-6 p-4 bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-900/60 rounded-lg text-sm text-amber-800 dark:text-amber-200">
                    目前是 Anthropic API 模式但尚未設定 API Key——請點右上角齒輪填入,或切換為本機 Claude CLI 模式。
                </div>
            )}

            {podcasts.length === 0 && (
                <p className="text-ink-faint text-sm">沒有已轉譯的 podcast,請先下載並轉譯。</p>
            )}

            {pending.length > 0 && (
                <div className="mb-6">
                    <h2 className="text-lg font-semibold mb-3">待校正 ({pending.length})</h2>
                    <ul className="space-y-2">
                        {pending.map((d) => (
                            <li key={d.name} className="px-4 py-3 bg-card rounded-lg border border-edge text-sm">
                                <div className="flex items-center justify-between gap-3">
                                    <span className="flex-1">{d.name}</span>
                                    {renderAction(d)}
                                    {renderDelete(d)}
                                </div>
                                {renderResult(d)}
                            </li>
                        ))}
                    </ul>
                </div>
            )}

            {done.length > 0 && (
                <div>
                    <h2 className="text-lg font-semibold mb-3">已校正 ({done.length})</h2>
                    <ul className="space-y-2">
                        {done.map((d) => (
                            <li key={d.name} className="px-4 py-3 bg-purple-50 dark:bg-purple-950/40 rounded-lg border border-purple-200 dark:border-purple-900/60 text-sm">
                                <div className="flex items-center justify-between gap-3">
                                    <span className="flex-1">{d.name}</span>
                                    {renderAction(d)}
                                    {renderDelete(d)}
                                </div>
                                {renderResult(d)}
                            </li>
                        ))}
                    </ul>
                </div>
            )}

            <ConfirmDeleteDialog name={confirmDelete} onCancel={() => setConfirmDelete('')} onConfirm={handleDelete} />
        </div>
    )
}

export default Correct
