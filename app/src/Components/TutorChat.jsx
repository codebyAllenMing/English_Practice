import { useState, useEffect, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'

// 練習頁 AI 助教:依集隔離、anchorLine 自動夾帶(「這句」有所指)。
// 歷史對話存 Messages 表(跨 session 記憶);CLI 模式一次問答約十幾秒。
function TutorChat({ folder, anchorLine }) {
    const [open, setOpen] = useState(false)
    const [msgs, setMsgs] = useState([])
    const [input, setInput] = useState('')
    const [asking, setAsking] = useState(false)
    const [error, setError] = useState('')
    const bottomRef = useRef(null)

    useEffect(() => {
        if (open) invoke('list_messages', { folder }).then(setMsgs).catch(() => {})
    }, [open, folder])

    useEffect(() => {
        bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
    }, [msgs, asking])

    const handleSend = async () => {
        const q = input.trim()
        if (!q || asking) return
        setAsking(true)
        setError('')
        setInput('')
        setMsgs((m) => [...m, { role: 'user', content: q, anchorLine }])
        try {
            const res = await invoke('ask_tutor', { folder, question: q, anchorLine })
            setMsgs((m) => [...m, { role: 'assistant', content: res.answer, anchorLine }])
        } catch (err) {
            setError(String(err))
        } finally {
            setAsking(false)
        }
    }

    return (
        <>
            <button
                className="fixed bottom-6 left-6 z-40 w-12 h-12 rounded-full bg-primary text-white shadow-lg hover:bg-primary-hover text-xl"
                onClick={() => setOpen(!open)}
                title="問 AI 助教(針對本集)"
            >
                🤖
            </button>
            {open && (
                <div className="fixed bottom-20 left-6 z-40 w-[380px] max-h-[70vh] bg-surface border border-edge rounded-xl shadow-xl flex flex-col">
                    <div className="px-4 py-3 border-b border-edge flex items-center justify-between shrink-0">
                        <p className="text-sm font-bold">AI 助教<span className="text-xs text-ink-faint font-normal ml-2">只回答本集內容</span></p>
                        <button className="text-ink-faint hover:text-ink-soft" onClick={() => setOpen(false)}>✕</button>
                    </div>
                    <div className="flex-1 overflow-y-auto p-3 space-y-2 min-h-[140px]">
                        {msgs.length === 0 && !asking && (
                            <p className="text-xs text-ink-faint text-center py-6">
                                針對這一集發問,例如:<br />「這句的文法是什麼?」「L34 那個用法口語嗎?」
                            </p>
                        )}
                        {msgs.map((m, i) => (
                            <div
                                key={i}
                                className={`text-sm p-2.5 rounded-lg whitespace-pre-wrap ${
                                    m.role === 'user' ? 'bg-muted ml-8' : 'bg-card border border-edge mr-8'
                                }`}
                            >
                                {m.role === 'user' && m.anchorLine != null && (
                                    <span className="block text-[11px] text-ink-faint mb-1">於第 {m.anchorLine} 句</span>
                                )}
                                {m.content}
                            </div>
                        ))}
                        {asking && <p className="text-xs text-ink-faint">思考中…(CLI 模式約十幾秒)</p>}
                        {error && <p className="text-xs text-red-500 dark:text-red-400">{error}</p>}
                        <div ref={bottomRef} />
                    </div>
                    <div className="p-3 border-t border-edge flex gap-2 items-center shrink-0">
                        <span className="text-[11px] text-ink-faint shrink-0" title="提問會自動夾帶目前句">L{anchorLine}</span>
                        <input
                            className="flex-1 px-3 py-2 text-sm bg-card border border-edge rounded-lg focus:outline-none focus:border-primary"
                            value={input}
                            onChange={(e) => setInput(e.target.value)}
                            onKeyDown={(e) => {
                                if (e.key === 'Enter') handleSend()
                            }}
                            placeholder="問這一集的問題…"
                            disabled={asking}
                        />
                        <button
                            className="px-3 py-2 bg-primary text-white text-sm rounded-lg hover:bg-primary-hover disabled:opacity-50"
                            onClick={handleSend}
                            disabled={asking || !input.trim()}
                        >
                            送出
                        </button>
                    </div>
                </div>
            )}
        </>
    )
}

export default TutorChat
