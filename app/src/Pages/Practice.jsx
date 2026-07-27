import { useState, useEffect, useRef, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useLoading } from '../Hooks/useLoading'
import VoiceDialog from '../Components/VoiceDialog'

// 講者 tag 色盤:依出場順序輪流分配,同名固定同色
const SPEAKER_COLORS = [
    'bg-blue-100 dark:bg-blue-900/50 text-blue-700 dark:text-blue-300',
    'bg-rose-100 dark:bg-rose-900/50 text-rose-700 dark:text-rose-300',
    'bg-emerald-100 dark:bg-emerald-900/50 text-emerald-700 dark:text-emerald-300',
    'bg-amber-100 text-amber-700',
    'bg-violet-100 dark:bg-violet-900/50 text-violet-700 dark:text-violet-300',
    'bg-cyan-100 dark:bg-cyan-900/50 text-cyan-700 dark:text-cyan-300',
]

function Practice() {
    const [podcasts, setPodcasts] = useState([])
    const [selected, setSelected] = useState('')
    const [view, setView] = useState('') // '' = 清單 | 'practice' = 逐句練習 | 'read' = 純文字閱讀
    const [lines, setLines] = useState([])
    const [currentIndex, setCurrentIndex] = useState(0)
    const [playing, setPlaying] = useState(false)
    const [currentData, setCurrentData] = useState(null)
    const [error, setError] = useState('')
    const [ready, setReady] = useState(false)
    const [fontSize, setFontSize] = useState(24)
    const [voiceFolder, setVoiceFolder] = useState('')
    const audioRef = useRef(null)
    const listRef = useRef(null)
    const lineRefs = useRef([])
    const loading = useLoading()

    useEffect(() => {
        invoke('list_transcribed').then(setPodcasts).catch(console.error)
    }, [])

    const speakerColors = useMemo(() => {
        const map = {}
        let idx = 0
        for (const line of lines) {
            const m = line.match(/^\[([^\]]+)\]:/)
            if (m && !(m[1] in map)) {
                map[m[1]] = SPEAKER_COLORS[idx % SPEAKER_COLORS.length]
                idx++
            }
        }
        return map
    }, [lines])

    const handlePractice = async (name) => {
        setSelected(name)
        setView('practice')
        setCurrentIndex(0)
        setCurrentData(null)
        setReady(false)
        setError('')
        loading(true, '載入語音模型...')
        try {
            const l = await invoke('get_lines', { folder: name })
            setLines(l)
            await invoke('start_practice')
            setReady(true)
        } catch (err) {
            setError(String(err))
        } finally {
            loading(false)
        }
    }

    const handleRead = async (name) => {
        setSelected(name)
        setView('read')
        setError('')
        try {
            const l = await invoke('get_lines', { folder: name })
            setLines(l)
        } catch (err) {
            setError(String(err))
        }
    }

    const handleBack = async () => {
        if (view === 'practice') {
            await invoke('stop_practice').catch(console.error)
        }
        setSelected('')
        setView('')
        setLines([])
        setCurrentData(null)
        setReady(false)
    }

    const playAt = async (index) => {
        if (index < 0 || index >= lines.length || playing || !ready) return
        setCurrentIndex(index)
        setPlaying(true)
        setError('')
        try {
            const result = await invoke('play_line', { folder: selected, index })
            setCurrentData(result)
            const audio = new Audio(`data:audio/wav;base64,${result.audio}`)
            audioRef.current = audio
            audio.onended = () => setPlaying(false)
            audio.play()
        } catch (err) {
            setError(String(err))
            setPlaying(false)
        }
    }

    const handlePrev = () => playAt(currentIndex - 1)
    const handleNext = () => playAt(currentIndex + 1)
    const handleRepeat = () => playAt(currentIndex)

    useEffect(() => {
        const el = lineRefs.current[currentIndex]
        const container = listRef.current
        if (el && container) {
            const offsetTop = el.offsetTop - container.offsetTop
            container.scrollTo({ top: offsetTop, behavior: 'smooth' })
        }
    }, [currentIndex])

    useEffect(() => {
        const handleKeyDown = (e) => {
            if (view !== 'practice' || !selected || lines.length === 0) return
            if (e.key === 'ArrowUp') { e.preventDefault(); handlePrev() }
            if (e.key === 'ArrowDown' || e.key === ' ') { e.preventDefault(); handleNext() }
            if (e.key === 'ArrowLeft') { e.preventDefault(); handleRepeat() }
        }
        window.addEventListener('keydown', handleKeyDown)
        return () => window.removeEventListener('keydown', handleKeyDown)
        // deps 已涵蓋 handlePrev/Next/Repeat 實際讀取的 state；把它們本身加進來會每次 render 重綁
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [view, selected, lines, currentIndex, playing, ready])

    const fontSizeControls = (
        <div className="flex items-center gap-1">
            <button
                className="w-8 h-8 flex items-center justify-center rounded-md text-ink-faint hover:bg-muted text-sm font-bold"
                onClick={() => setFontSize(s => Math.max(14, s - 2))}
                title="縮小字體"
            >
                A-
            </button>
            <button
                className="w-8 h-8 flex items-center justify-center rounded-md text-ink-faint hover:bg-muted text-lg font-bold"
                onClick={() => setFontSize(s => Math.min(48, s + 2))}
                title="放大字體"
            >
                A+
            </button>
        </div>
    )

    // ── 清單:正方形卡片格 ──
    if (!selected) {
        return (
            <div>
                <h1 className="text-2xl font-bold mb-6">練習</h1>
                {error && <p className="text-sm text-red-500 dark:text-red-400 mb-4">{error}</p>}
                {podcasts.length === 0 ? (
                    <p className="text-ink-faint text-sm">沒有可練習的 podcast</p>
                ) : (
                    <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4">
                        {podcasts.map((name) => (
                            <div
                                key={name}
                                className="aspect-square bg-card rounded-xl border border-edge p-4 flex flex-col hover:border-edge-strong hover:shadow-sm transition-all"
                            >
                                <div className="flex-1 flex items-center justify-center text-center text-sm font-medium text-ink-soft break-all overflow-hidden">
                                    {name}
                                </div>
                                <div className="flex gap-2 mt-3">
                                    <button
                                        className="flex-1 px-2 py-1.5 text-xs bg-primary text-white rounded-md hover:bg-primary-hover"
                                        onClick={() => handlePractice(name)}
                                    >
                                        ▶ 練習
                                    </button>
                                    <button
                                        className="flex-1 px-2 py-1.5 text-xs text-ink-soft border border-edge-strong rounded-md hover:bg-muted"
                                        onClick={() => handleRead(name)}
                                    >
                                        📄 文字
                                    </button>
                                    <button
                                        className="px-2 py-1.5 text-xs text-ink-soft border border-edge-strong rounded-md hover:bg-muted shrink-0"
                                        onClick={() => setVoiceFolder(name)}
                                        title="聲音設定"
                                    >
                                        🔊
                                    </button>
                                </div>
                            </div>
                        ))}
                    </div>
                )}
                {voiceFolder && <VoiceDialog folder={voiceFolder} onClose={() => setVoiceFolder('')} />}
            </div>
        )
    }

    // ── 純文字閱讀模式 ──
    // h-full + 內層 overflow:標題列固定,只有逐字稿區塊捲動
    if (view === 'read') {
        return (
            <div className="h-full flex flex-col">
                <div className="flex items-center gap-3 pb-3 mb-1 border-b border-edge shrink-0">
                    <button
                        className="text-sm text-ink-faint hover:text-ink-soft shrink-0"
                        onClick={handleBack}
                    >
                        ← 返回
                    </button>
                    <h1 className="text-2xl font-bold flex-1 break-all">{selected}</h1>
                    {fontSizeControls}
                </div>

                {error && <p className="text-sm text-red-500 dark:text-red-400 my-2 shrink-0">{error}</p>}

                <div className="flex-1 overflow-y-auto">
                <div className="divide-y divide-edge">
                    {lines.map((line, i) => {
                        const m = line.match(/^\[([^\]]+)\]:\s*(.*)$/)
                        const speaker = m ? m[1] : ''
                        const text = m ? m[2] : line
                        return (
                            <div key={i} className="flex gap-3 items-baseline py-3">
                                <span className="text-ink-faint/60 select-none w-8 text-right shrink-0 text-sm">{i + 1}</span>
                                <span
                                    className={`text-xs font-medium w-20 shrink-0 truncate text-center px-2 py-0.5 rounded-full ${speakerColors[speaker] || 'bg-muted text-ink-faint'}`}
                                    title={speaker}
                                >
                                    {speaker || '—'}
                                </span>
                                <p className="flex-1 leading-relaxed" style={{ fontSize: `${fontSize}px` }}>{text}</p>
                            </div>
                        )
                    })}
                </div>
                </div>
            </div>
        )
    }

    // ── 逐句練習模式 ──
    return (
        <div>
            <div className="flex items-center gap-3 mb-6">
                <button
                    className="text-sm text-ink-faint hover:text-ink-soft"
                    onClick={handleBack}
                >
                    ← 返回
                </button>
                <h1 className="text-2xl font-bold flex-1">{selected}</h1>
                {fontSizeControls}
            </div>

            {error && (
                <p className="text-sm text-red-500 dark:text-red-400 mb-4">{error}</p>
            )}

            <div className="mb-6 p-6 bg-card rounded-lg border border-edge min-h-[180px] flex flex-col justify-between">
                <div className="flex-1 flex flex-col justify-center">
                    {currentData ? (
                        <>
                            <p className="text-sm text-ink-faint mb-3">[{currentData.speaker}] ({currentData.index + 1}/{currentData.total})</p>
                            <p className="leading-relaxed" style={{ fontSize: `${fontSize}px` }}>{currentData.text}</p>
                        </>
                    ) : (
                        <p className="text-ink-faint text-center">按下方按鈕或鍵盤開始播放</p>
                    )}
                </div>

                <div className="flex items-center justify-center gap-4 mt-6 pt-4 border-t border-edge">
                    <button
                        className="px-4 py-2 bg-muted rounded-lg text-sm hover:bg-muted-strong disabled:opacity-50"
                        onClick={handlePrev}
                        disabled={playing || currentIndex <= 0 || !ready}
                        title="上一句 (↑)"
                    >
                        ↑ 上一句
                    </button>
                    <button
                        className="px-4 py-2 bg-muted rounded-lg text-sm hover:bg-muted-strong disabled:opacity-50"
                        onClick={handleRepeat}
                        disabled={playing || !ready}
                        title="重複 (←)"
                    >
                        ← 重複
                    </button>
                    <button
                        className="px-6 py-2 bg-primary text-white rounded-lg text-sm hover:bg-primary-hover disabled:opacity-50"
                        onClick={handleNext}
                        disabled={playing || currentIndex >= lines.length - 1 || !ready}
                        title="下一句 (↓ / 空白鍵)"
                    >
                        下一句 ↓
                    </button>
                </div>
            </div>

            <p className="text-xs text-ink-faint text-center">
                鍵盤：↑ 上一句  ← 重複  ↓/空白 下一句
            </p>

            <div className="mt-6 max-h-[300px] overflow-y-auto" ref={listRef}>
                <ul className="space-y-1">
                    {lines.map((line, i) => (
                        <li
                            key={i}
                            ref={el => lineRefs.current[i] = el}
                            className={`px-3 py-2 rounded text-sm cursor-pointer flex gap-3 ${i === currentIndex ? 'bg-blue-50 dark:bg-blue-950/40 border border-blue-200 dark:border-blue-900/60 text-blue-800 dark:text-blue-300' : 'text-ink-faint hover:bg-card'}`}
                            onClick={() => playAt(i)}
                        >
                            <span className="text-ink-faint/60 select-none w-8 text-right shrink-0">{i + 1}</span>
                            <span>{line}</span>
                        </li>
                    ))}
                </ul>
            </div>
        </div>
    )
}

export default Practice
