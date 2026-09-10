import { useState, useEffect, useRef, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useLoading } from '../Hooks/useLoading'
import VoiceDialog from '../Components/VoiceDialog'
import TutorChat from '../Components/TutorChat'

// 講者 tag 色盤:依出場順序輪流分配,同名固定同色
const SPEAKER_COLORS = [
    'bg-blue-100 dark:bg-blue-900/50 text-blue-700 dark:text-blue-300',
    'bg-rose-100 dark:bg-rose-900/50 text-rose-700 dark:text-rose-300',
    'bg-emerald-100 dark:bg-emerald-900/50 text-emerald-700 dark:text-emerald-300',
    'bg-amber-100 text-amber-700',
    'bg-violet-100 dark:bg-violet-900/50 text-violet-700 dark:text-violet-300',
    'bg-cyan-100 dark:bg-cyan-900/50 text-cyan-700 dark:text-cyan-300',
]

// 詞級振り仮名渲染:segs = [[詞面, 讀音|null], ...];無標音資料時原樣顯示 fallback
function RubyText({ segs, fallback }) {
    if (!segs || segs.length === 0) return fallback
    return segs.map(([surface, reading], i) =>
        reading ? (
            <ruby key={i}>
                {surface}
                <rt className="text-[0.45em] text-ink-faint font-normal">{reading}</rt>
            </ruby>
        ) : (
            <span key={i}>{surface}</span>
        )
    )
}

// 可點標記的當前句:點詞 toggle 生字(en 依空白切詞、ja 用 lindera 斷詞的 ruby segs)
function MarkableText({ text, ruby, lineNo, markedSet, onToggle }) {
    const strip = (s) => s.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, '')
    const cls = (marked) =>
        `cursor-pointer rounded px-0.5 hover:bg-amber-100/70 dark:hover:bg-amber-900/40 ${
            marked ? 'bg-amber-200/70 dark:bg-amber-800/50' : ''
        }`
    if (ruby && ruby.length > 0) {
        return ruby.map(([surface, reading], i) => {
            const term = strip(surface)
            if (!term) return <span key={i}>{surface}</span>
            const marked = markedSet.has(`${lineNo}:${term}`)
            return (
                <span key={i} className={cls(marked)} onClick={() => onToggle(term, reading || '')}>
                    {reading ? (
                        <ruby>
                            {surface}
                            <rt className="text-[0.45em] text-ink-faint font-normal">{reading}</rt>
                        </ruby>
                    ) : (
                        surface
                    )}
                </span>
            )
        })
    }
    return (text || '').split(/(\s+)/).map((tok, i) => {
        const term = strip(tok)
        if (!term) return <span key={i}>{tok}</span>
        const marked = markedSet.has(`${lineNo}:${term}`)
        return (
            <span key={i} className={cls(marked)} onClick={() => onToggle(term, '')}>
                {tok}
            </span>
        )
    })
}

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
    const [jaDownloading, setJaDownloading] = useState(false) // VOICEVOX 引擎下載中
    const [jaProgress, setJaProgress] = useState(null) // {received, total}
    const [rubyLines, setRubyLines] = useState([]) // 閱讀模式的逐行振り仮名(非日文課綱為空)
    const [analysis, setAnalysis] = useState(null) // 講義:null=未載入 | {exists:false} | {exists:true, result}
    const [analyzing, setAnalyzing] = useState(false)
    const [outlineOpen, setOutlineOpen] = useState(false)
    const [vocabMarks, setVocabMarks] = useState([]) // 本集已標記生字(高亮用)
    const [wordCard, setWordCard] = useState(null) // 點詞後的詞卡 {term, reading, meaning, loading}
    const [allVocab, setAllVocab] = useState([]) // 清單頁生字本(目前課綱全部)
    const [vocabOpen, setVocabOpen] = useState(false)
    const audioRef = useRef(null)
    const listRef = useRef(null)
    const lineRefs = useRef([])
    const loading = useLoading()

    useEffect(() => {
        invoke('list_transcribed').then(setPodcasts).catch(console.error)
    }, [])

    // 生字本:回到清單頁時重載(練習中新標記的會出現)
    useEffect(() => {
        if (!selected) invoke('list_vocab', { folder: null }).then(setAllVocab).catch(() => {})
    }, [selected])

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

    // 詞彙依行號索引:{行號: [詞彙...]};播放到該句時顯示「本句重點」,清單行號旁標圓點
    const vocabByLine = useMemo(() => {
        const map = {}
        for (const v of analysis?.result?.vocab || []) {
            ;(map[v.line] = map[v.line] || []).push(v)
        }
        return map
    }, [analysis])

    const markedSet = useMemo(() => new Set(vocabMarks.map((v) => `${v.lineNo}:${v.term}`)), [vocabMarks])

    // 單詞發音(ja=VOICEVOX、en=kokoro);失敗不擋收藏
    const playTerm = async (term) => {
        try {
            const res = await invoke('speak_term', { term })
            new Audio(`data:audio/wav;base64,${res.audio}`).play().catch(() => {})
        } catch (err) {
            console.error(err)
        }
    }

    // 點句中詞:發音 + 彈詞卡(詞性/釋義 AI 即查一次,Lookups 表終身快取);收藏改為卡上 ☆ 顯式動作
    const handleWordClick = async (term, reading = '') => {
        if (!currentData) return
        const lineNo = currentData.index + 1
        playTerm(term)
        const hit = (analysis?.result?.vocab || []).find((x) => x.term === term)
        const saved = vocabMarks.find((v) => v.lineNo === lineNo && v.term === term)
        const known = hit?.meaning || saved?.meaning || ''
        setWordCard({ term, reading, lineNo, meaning: known, loading: !known })
        if (known) return
        // 詞性本地即查(ja lindera,毫秒級),AI 釋義到達前先亮出來
        invoke('term_pos', { term })
            .then((pos) => {
                if (pos) setWordCard((c) => (c?.term === term && c.loading ? { ...c, pos } : c))
            })
            .catch(() => {})
        try {
            const res = await invoke('lookup_term', { folder: selected, term, lineNo })
            setWordCard((c) => (c?.term === term ? { ...c, meaning: res.combined, loading: false } : c))
        } catch (err) {
            setWordCard((c) => (c?.term === term ? { ...c, meaning: `查詢失敗：${err}`, loading: false } : c))
        }
    }

    // 詞卡 ☆:顯式收藏/取消(meaning 帶詞卡查得的釋義)
    const handleCardSave = async () => {
        if (!wordCard) return
        try {
            await invoke('toggle_vocab', {
                folder: selected,
                lineNo: wordCard.lineNo,
                term: wordCard.term,
                reading: wordCard.reading || '',
                meaning: wordCard.loading ? '' : wordCard.meaning || '',
                source: 'user',
            })
            setVocabMarks(await invoke('list_vocab', { folder: selected }))
        } catch (err) {
            setError(String(err))
        }
    }

    // 講義卡 ☆:收藏(meaning 現成,不彈卡)
    const handleStarVocab = async (v) => {
        try {
            await invoke('toggle_vocab', {
                folder: selected,
                lineNo: v.line,
                term: v.term,
                reading: v.reading || '',
                meaning: v.meaning || '',
                source: 'analysis',
            })
            setVocabMarks(await invoke('list_vocab', { folder: selected }))
        } catch (err) {
            setError(String(err))
        }
    }

    const handleRemoveVocab = async (v) => {
        try {
            await invoke('toggle_vocab', { folder: v.folder, lineNo: v.lineNo, term: v.term })
            setAllVocab(await invoke('list_vocab', { folder: null }))
        } catch (err) {
            setError(String(err))
        }
    }

    const handlePractice = async (name) => {
        setSelected(name)
        setView('practice')
        setCurrentIndex(0)
        setCurrentData(null)
        setReady(false)
        setError('')
        setAnalysis(null)
        setOutlineOpen(false)
        invoke('get_analysis', { folder: name }).then(setAnalysis).catch(() => {})
        invoke('list_vocab', { folder: name }).then(setVocabMarks).catch(() => {})
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

    // 手動生成講義(舊集補生成/失敗重試;校正流程會自動跑,多數情況看不到這顆按鈕)
    const handleAnalyze = async () => {
        setAnalyzing(true)
        setError('')
        try {
            await invoke('analyze_transcript', { folder: selected })
            setAnalysis(await invoke('get_analysis', { folder: selected }))
        } catch (err) {
            setError(`講義生成失敗：${err}`)
        } finally {
            setAnalyzing(false)
        }
    }

    // 日文課綱首次練習:下載 VOICEVOX 引擎(1.8GB)後自動重試啟動
    const handleDownloadJa = async () => {
        setJaDownloading(true)
        setError('')
        const unlisten = await listen('model-progress', (e) => setJaProgress(e.payload))
        try {
            await invoke('download_voicevox')
            loading(true, '載入語音引擎...')
            await invoke('start_practice')
            setReady(true)
        } catch (err) {
            setError(String(err))
        } finally {
            unlisten()
            loading(false)
            setJaDownloading(false)
            setJaProgress(null)
        }
    }

    const handleRead = async (name) => {
        setSelected(name)
        setView('read')
        setError('')
        try {
            const l = await invoke('get_lines', { folder: name })
            setLines(l)
            // 日文課綱回逐行標音;英文回空陣列,原樣渲染
            setRubyLines(await invoke('get_ruby', { folder: name }).catch(() => []))
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

    // playing 期間鎖住換句(含按鈕 disabled):刻意設計,避免連續按
    const playAt = async (index) => {
        if (index < 0 || index >= lines.length || playing || !ready) return
        setCurrentIndex(index)
        setPlaying(true)
        setError('')
        setWordCard(null)
        try {
            const result = await invoke('play_line', { folder: selected, index })
            setCurrentData(result)
            const audio = new Audio(`data:audio/wav;base64,${result.audio}`)
            audioRef.current = audio
            audio.onended = () => setPlaying(false)
            audio.onerror = () => {
                setError(`第 ${index + 1} 句音訊解碼失敗`)
                setPlaying(false)
                invoke('log_ui', { msg: `第 ${index + 1} 句 audio element 解碼失敗` }).catch(() => {})
            }
            // play() 的 rejection 要接住:合成 await 之後手勢授權可能已失效(WebKit),
            // 不接的話 playing 卡 true、整頁靜音且無任何線索
            await audio.play()
        } catch (err) {
            setError(`第 ${index + 1} 句播放失敗:${err}`)
            setPlaying(false)
            invoke('log_ui', { msg: `第 ${index + 1} 句播放失敗: ${err}` }).catch(() => {})
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
            // 聊天輸入框打字時不觸發快捷鍵(空白鍵=下一句的衝突防護)
            if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return
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
                <div className="mt-8">
                    <button
                        className="w-full text-left text-sm text-ink-soft px-3 py-2 bg-card border border-edge rounded-lg hover:bg-muted"
                        onClick={() => setVocabOpen(!vocabOpen)}
                    >
                        {vocabOpen ? '▾' : '▸'} 生字本（{allVocab.length}）
                    </button>
                    {vocabOpen &&
                        (allVocab.length === 0 ? (
                            <p className="text-xs text-ink-faint mt-2 px-1">練習時點句子裡的單字、或按講義卡的 ☆ 即可收藏</p>
                        ) : (
                            <div className="mt-2 divide-y divide-edge bg-card border border-edge rounded-lg">
                                {allVocab.map((v) => (
                                    <div key={v.id} className="flex items-baseline gap-3 px-4 py-2.5 text-sm">
                                        <span className="font-bold shrink-0">{v.term}</span>
                                        {v.reading && <span className="text-ink-faint text-xs shrink-0">（{v.reading}）</span>}
                                        <span className="text-ink-soft flex-1 text-xs truncate">{v.meaning}</span>
                                        <span className="text-ink-faint text-xs shrink-0">{v.folder} L{v.lineNo}</span>
                                        <button
                                            className="text-ink-faint/60 hover:text-ink-soft shrink-0 text-xs"
                                            title="發音"
                                            onClick={() => playTerm(v.term)}
                                        >
                                            🔊
                                        </button>
                                        <button
                                            className="text-ink-faint/60 hover:text-red-500 dark:hover:text-red-400 shrink-0 text-xs"
                                            title="移除"
                                            onClick={() => handleRemoveVocab(v)}
                                        >
                                            ✕
                                        </button>
                                    </div>
                                ))}
                            </div>
                        ))}
                </div>
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
                                <p
                                    className={`flex-1 ${rubyLines[i]?.length ? 'leading-[2.1]' : 'leading-relaxed'}`}
                                    style={{ fontSize: `${fontSize}px` }}
                                >
                                    <RubyText segs={rubyLines[i]} fallback={text} />
                                </p>
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
                <div className="mb-4">
                    <p className="text-sm text-red-500 dark:text-red-400">{error}</p>
                    {error.includes('需要下載日文語音引擎') && (
                        <button
                            className="mt-2 px-4 py-2 bg-primary text-white text-sm rounded-lg hover:bg-primary-hover"
                            onClick={handleDownloadJa}
                        >
                            下載日文語音引擎(1.8GB,一次性)
                        </button>
                    )}
                </div>
            )}

            {jaDownloading && (
                <div className="mb-4">
                    <p className="text-sm text-blue-600 dark:text-blue-400 mb-2">
                        {jaProgress
                            ? `下載語音引擎中... ${Math.round(jaProgress.received / 1048576)} / ${Math.round(jaProgress.total / 1048576)} MB`
                            : '連線中...'}
                    </p>
                    <div className="w-full bg-muted rounded-full h-2">
                        <div
                            className="bg-primary h-2 rounded-full transition-all duration-300"
                            style={{ width: `${jaProgress && jaProgress.total > 0 ? Math.min(100, Math.round(jaProgress.received / jaProgress.total * 100)) : 0}%` }}
                        />
                    </div>
                </div>
            )}

            <div className="mb-6 p-6 bg-card rounded-lg border border-edge min-h-[180px] flex flex-col justify-between">
                <div className="flex-1 flex flex-col justify-center">
                    {currentData ? (
                        <>
                            <p className="text-sm text-ink-faint mb-3">
                                [{currentData.speaker}] ({currentData.index + 1}/{currentData.total}){playing && <span className="ml-2 text-primary">▶ 播放中</span>}
                            </p>
                            <p
                                className={currentData.ruby?.length ? 'leading-[2.1]' : 'leading-relaxed'}
                                style={{ fontSize: `${fontSize}px` }}
                                title="點單字:發音+詞性釋義;詞卡上 ☆ 收藏"
                            >
                                <MarkableText
                                    text={currentData.text}
                                    ruby={currentData.ruby}
                                    lineNo={currentData.index + 1}
                                    markedSet={markedSet}
                                    onToggle={handleWordClick}
                                />
                            </p>
                            {wordCard && (
                                <div className="mt-3 p-3 rounded-lg bg-card border border-edge text-sm flex items-start gap-2">
                                    <span className="font-bold shrink-0">{wordCard.term}</span>
                                    {wordCard.reading && (
                                        <span className="text-ink-faint text-xs shrink-0 mt-0.5">（{wordCard.reading}）</span>
                                    )}
                                    <span className="text-ink-soft flex-1">
                                        {wordCard.loading
                                            ? wordCard.pos
                                                ? `【${wordCard.pos}】查詢釋義中…`
                                                : '查詢詞性與釋義中…'
                                            : wordCard.meaning}
                                    </span>
                                    <button
                                        className="text-base text-amber-500 hover:scale-110 transition-transform shrink-0"
                                        title="加入/移除生字本"
                                        onClick={handleCardSave}
                                    >
                                        {markedSet.has(`${wordCard.lineNo}:${wordCard.term}`) ? '★' : '☆'}
                                    </button>
                                    <button
                                        className="text-ink-faint hover:text-ink-soft shrink-0"
                                        title="再唸一次"
                                        onClick={() => playTerm(wordCard.term)}
                                    >
                                        🔊
                                    </button>
                                    <button
                                        className="text-ink-faint/60 hover:text-ink-soft shrink-0"
                                        onClick={() => setWordCard(null)}
                                    >
                                        ✕
                                    </button>
                                </div>
                            )}
                            {(vocabByLine[currentData.index + 1] || []).length > 0 && (
                                <div className="mt-4 space-y-2">
                                    <p className="text-xs text-ink-faint">本句重點</p>
                                    {vocabByLine[currentData.index + 1].map((v, i) => (
                                        <div
                                            key={i}
                                            className="relative p-3 rounded-lg bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-900/40"
                                        >
                                            <div className="absolute top-2 right-2 flex items-center gap-1.5">
                                                <button
                                                    className="text-sm text-ink-faint hover:text-ink-soft"
                                                    title="發音"
                                                    onClick={() => playTerm(v.term)}
                                                >
                                                    🔊
                                                </button>
                                                <button
                                                    className="text-base text-amber-500 hover:scale-110 transition-transform"
                                                    title="收藏到生字本"
                                                    onClick={() => handleStarVocab(v)}
                                                >
                                                    {markedSet.has(`${v.line}:${v.term}`) ? '★' : '☆'}
                                                </button>
                                            </div>
                                            <p className="text-sm pr-6">
                                                <span className="font-bold">{v.term}</span>
                                                {v.reading && <span className="text-ink-faint ml-1">（{v.reading}）</span>}
                                                <span className="ml-2 text-xs px-1.5 py-0.5 rounded bg-amber-200/60 dark:bg-amber-800/60 text-amber-900 dark:text-amber-200">
                                                    {v.type}
                                                </span>
                                            </p>
                                            <p className="text-sm text-ink-soft mt-1">{v.meaning}</p>
                                        </div>
                                    ))}
                                </div>
                            )}
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

            {analysis && !analysis.exists && (
                <div className="mt-4 flex items-center justify-center gap-3 text-sm text-ink-faint">
                    本集尚無講義
                    <button
                        className="px-3 py-1.5 bg-muted rounded-lg hover:bg-muted-strong text-ink-soft disabled:opacity-50"
                        onClick={handleAnalyze}
                        disabled={analyzing}
                    >
                        {analyzing ? '生成講義中…（約 1～2 分鐘）' : '生成講義'}
                    </button>
                </div>
            )}

            {analysis?.exists && analysis.result?.sections?.length > 0 && (
                <div className="mt-4">
                    <button
                        className="w-full text-left text-sm text-ink-soft px-3 py-2 bg-card border border-edge rounded-lg hover:bg-muted"
                        onClick={() => setOutlineOpen(!outlineOpen)}
                    >
                        {outlineOpen ? '▾' : '▸'} 大綱
                        {!outlineOpen && (
                            <span className="text-ink-faint ml-2">
                                {analysis.result.sections.map((s) => s.title).join(' · ')}
                            </span>
                        )}
                    </button>
                    {outlineOpen && (
                        <div className="mt-2 p-3 bg-card border border-edge rounded-lg">
                            <p className="text-sm text-ink-soft mb-3">{analysis.result.summary}</p>
                            <div className="space-y-1">
                                {analysis.result.sections.map((s, i) => (
                                    <button
                                        key={i}
                                        className="w-full text-left text-sm px-2 py-1.5 rounded hover:bg-muted flex justify-between"
                                        onClick={() => playAt(s.startLine - 1)}
                                    >
                                        <span>{s.title}</span>
                                        <span className="text-ink-faint">L{s.startLine}–{s.endLine} →</span>
                                    </button>
                                ))}
                            </div>
                        </div>
                    )}
                </div>
            )}

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
                            <span className="w-2 shrink-0 self-center">
                                {vocabByLine[i + 1] && <span className="block w-1.5 h-1.5 rounded-full bg-amber-400" title="本句有講點" />}
                            </span>
                            <span>{line}</span>
                        </li>
                    ))}
                </ul>
            </div>

            <TutorChat folder={selected} anchorLine={currentIndex + 1} />
        </div>
    )
}

export default Practice
