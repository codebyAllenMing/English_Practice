import { useState, useEffect } from 'react'
import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom'
import { invoke } from '@tauri-apps/api/core'
import { applyTheme, watchSystemTheme } from './theme'
import Download from './Pages/Download'
import Transcribe from './Pages/Transcribe'
import Correct from './Pages/Correct'
import Practice from './Pages/Practice'
import Settings from './Components/Settings'
import ModelSetup from './Components/ModelSetup'
import UpdateBanner from './Components/UpdateBanner'

function App() {
    const [showSettings, setShowSettings] = useState(false)
    const [lang, setLang] = useState('en') // 課綱語系,導覽列常駐顯示

    useEffect(() => {
        applyTheme()
        // WKWebView 預設把 Backspace 當「上一頁」,非輸入框狀態按 delete 會跳走路由——全域擋掉
        const blockBackspaceNav = (e) => {
            if (e.key !== 'Backspace') return
            const t = e.target
            const editable = t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable
            if (!editable) e.preventDefault()
        }
        window.addEventListener('keydown', blockBackspaceNav)
        const loadLang = () => {
            invoke('get_config')
                .then((config) => setLang(config.course_language || 'en'))
                .catch(() => {})
        }
        loadLang()
        window.addEventListener('config-saved', loadLang)
        const unwatch = watchSystemTheme()
        return () => {
            window.removeEventListener('keydown', blockBackspaceNav)
            window.removeEventListener('config-saved', loadLang)
            unwatch()
        }
    }, [])

    return (
        <BrowserRouter>
            <div className="h-screen flex flex-col">
            <nav className="flex items-center px-6 py-4 border-b border-edge bg-card shrink-0">
                <div className="flex gap-4 flex-1">
                    <NavLink to="/" className={({ isActive }) => `px-4 py-2 rounded-md text-sm ${isActive ? 'bg-muted text-ink font-bold' : 'text-ink-faint hover:text-ink-soft'}`}>
                        下載
                    </NavLink>
                    <NavLink to="/transcribe" className={({ isActive }) => `px-4 py-2 rounded-md text-sm ${isActive ? 'bg-muted text-ink font-bold' : 'text-ink-faint hover:text-ink-soft'}`}>
                        轉譯
                    </NavLink>
                    <NavLink to="/correct" className={({ isActive }) => `px-4 py-2 rounded-md text-sm ${isActive ? 'bg-muted text-ink font-bold' : 'text-ink-faint hover:text-ink-soft'}`}>
                        校正
                    </NavLink>
                    <NavLink to="/practice" className={({ isActive }) => `px-4 py-2 rounded-md text-sm ${isActive ? 'bg-muted text-ink font-bold' : 'text-ink-faint hover:text-ink-soft'}`}>
                        練習
                    </NavLink>
                </div>
                <span
                    className={`text-xs px-2.5 py-1 rounded-full mr-3 ${
                        lang === 'ja'
                            ? 'bg-amber-100 text-amber-800 dark:bg-amber-900/40 dark:text-amber-200'
                            : 'bg-muted text-ink-soft'
                    }`}
                    title="目前課綱語系(在下載頁切換)"
                >
                    課綱：{lang === 'ja' ? '日文' : '英文'}
                </span>
                <button
                    className="p-2 text-ink-faint hover:text-ink-soft rounded-md hover:bg-muted"
                    onClick={() => setShowSettings(true)}
                    title="設定"
                >
                    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth="2">
                        <path strokeLinecap="round" strokeLinejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                        <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                    </svg>
                </button>
            </nav>
            <main className="p-6 flex-1 overflow-y-auto">
                <Routes>
                    <Route path="/" element={<Download />} />
                    <Route path="/transcribe" element={<Transcribe />} />
                    <Route path="/correct" element={<Correct />} />
                    <Route path="/practice" element={<Practice />} />
                </Routes>
            </main>
            </div>
            {showSettings && <Settings onClose={() => setShowSettings(false)} />}
            <ModelSetup />
            <UpdateBanner />
        </BrowserRouter>
    )
}

export default App
