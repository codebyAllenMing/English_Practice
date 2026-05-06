import { useState, useEffect, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useLoading } from '../Hooks/useLoading'

function Practice() {
    const [podcasts, setPodcasts] = useState([])
    const [selected, setSelected] = useState('')
    const [lines, setLines] = useState([])
    const [currentIndex, setCurrentIndex] = useState(0)
    const [playing, setPlaying] = useState(false)
    const [currentData, setCurrentData] = useState(null)
    const [error, setError] = useState('')
    const [ready, setReady] = useState(false)
    const [fontSize, setFontSize] = useState(24)
    const audioRef = useRef(null)
    const listRef = useRef(null)
    const lineRefs = useRef([])
    const loading = useLoading()

    useEffect(() => {
        invoke('list_transcribed').then(setPodcasts).catch(console.error)
    }, [])

    const handleSelect = async (name) => {
        setSelected(name)
        setCurrentIndex(0)
        setCurrentData(null)
        setReady(false)
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

    const handleBack = async () => {
        await invoke('stop_practice').catch(console.error)
        setSelected('')
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
            if (!selected || lines.length === 0) return
            if (e.key === 'ArrowUp') { e.preventDefault(); handlePrev() }
            if (e.key === 'ArrowDown' || e.key === ' ') { e.preventDefault(); handleNext() }
            if (e.key === 'ArrowLeft') { e.preventDefault(); handleRepeat() }
        }
        window.addEventListener('keydown', handleKeyDown)
        return () => window.removeEventListener('keydown', handleKeyDown)
        // deps 已涵蓋 handlePrev/Next/Repeat 實際讀取的 state；把它們本身加進來會每次 render 重綁
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [selected, lines, currentIndex, playing, ready])

    if (!selected) {
        return (
            <div>
                <h1 className="text-2xl font-bold mb-6">練習</h1>
                {podcasts.length === 0 ? (
                    <p className="text-gray-400 text-sm">沒有可練習的 podcast</p>
                ) : (
                    <ul className="space-y-2">
                        {podcasts.map((name) => (
                            <li
                                key={name}
                                className="px-4 py-3 bg-gray-50 rounded-lg border border-gray-200 text-sm cursor-pointer hover:bg-gray-100"
                                onClick={() => handleSelect(name)}
                            >
                                {name}
                            </li>
                        ))}
                    </ul>
                )}
            </div>
        )
    }

    return (
        <div>
            <div className="flex items-center gap-3 mb-6">
                <button
                    className="text-sm text-gray-500 hover:text-gray-700"
                    onClick={handleBack}
                >
                    ← 返回
                </button>
                <h1 className="text-2xl font-bold flex-1">{selected}</h1>
                <div className="flex items-center gap-1">
                    <button
                        className="w-8 h-8 flex items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 text-sm font-bold"
                        onClick={() => setFontSize(s => Math.max(14, s - 2))}
                        title="縮小字體"
                    >
                        A-
                    </button>
                    <button
                        className="w-8 h-8 flex items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 text-lg font-bold"
                        onClick={() => setFontSize(s => Math.min(48, s + 2))}
                        title="放大字體"
                    >
                        A+
                    </button>
                </div>
            </div>

            {error && (
                <p className="text-sm text-red-500 mb-4">{error}</p>
            )}

            <div className="mb-6 p-6 bg-gray-50 rounded-lg border border-gray-200 min-h-[180px] flex flex-col justify-between">
                <div className="flex-1 flex flex-col justify-center">
                    {currentData ? (
                        <>
                            <p className="text-sm text-gray-400 mb-3">[{currentData.speaker}] ({currentData.index + 1}/{currentData.total})</p>
                            <p className="leading-relaxed" style={{ fontSize: `${fontSize}px` }}>{currentData.text}</p>
                        </>
                    ) : (
                        <p className="text-gray-400 text-center">按下方按鈕或鍵盤開始播放</p>
                    )}
                </div>

                <div className="flex items-center justify-center gap-4 mt-6 pt-4 border-t border-gray-200">
                    <button
                        className="px-4 py-2 bg-gray-200 rounded-lg text-sm hover:bg-gray-300 disabled:opacity-50"
                        onClick={handlePrev}
                        disabled={playing || currentIndex <= 0 || !ready}
                        title="上一句 (↑)"
                    >
                        ↑ 上一句
                    </button>
                    <button
                        className="px-4 py-2 bg-gray-200 rounded-lg text-sm hover:bg-gray-300 disabled:opacity-50"
                        onClick={handleRepeat}
                        disabled={playing || !ready}
                        title="重複 (←)"
                    >
                        ← 重複
                    </button>
                    <button
                        className="px-6 py-2 bg-gray-800 text-white rounded-lg text-sm hover:bg-gray-700 disabled:opacity-50"
                        onClick={handleNext}
                        disabled={playing || currentIndex >= lines.length - 1 || !ready}
                        title="下一句 (↓ / 空白鍵)"
                    >
                        下一句 ↓
                    </button>
                </div>
            </div>

            <p className="text-xs text-gray-400 text-center">
                鍵盤：↑ 上一句  ← 重複  ↓/空白 下一句
            </p>

            <div className="mt-6 max-h-[300px] overflow-y-auto" ref={listRef}>
                <ul className="space-y-1">
                    {lines.map((line, i) => (
                        <li
                            key={i}
                            ref={el => lineRefs.current[i] = el}
                            className={`px-3 py-2 rounded text-sm cursor-pointer flex gap-3 ${i === currentIndex ? 'bg-blue-50 border border-blue-200 text-blue-800' : 'text-gray-500 hover:bg-gray-50'}`}
                            onClick={() => playAt(i)}
                        >
                            <span className="text-gray-300 select-none w-8 text-right shrink-0">{i + 1}</span>
                            <span>{line}</span>
                        </li>
                    ))}
                </ul>
            </div>
        </div>
    )
}

export default Practice
