import { useState } from 'react'
import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom'
import Download from './Pages/Download'
import Transcribe from './Pages/Transcribe'
import Practice from './Pages/Practice'
import Settings from './Components/Settings'

function App() {
    const [showSettings, setShowSettings] = useState(false)

    return (
        <BrowserRouter>
            <nav className="flex items-center px-6 py-4 border-b border-gray-200 bg-gray-50">
                <div className="flex gap-4 flex-1">
                    <NavLink to="/" className={({ isActive }) => `px-4 py-2 rounded-md text-sm ${isActive ? 'bg-gray-200 text-gray-900 font-bold' : 'text-gray-500 hover:text-gray-700'}`}>
                        下載
                    </NavLink>
                    <NavLink to="/transcribe" className={({ isActive }) => `px-4 py-2 rounded-md text-sm ${isActive ? 'bg-gray-200 text-gray-900 font-bold' : 'text-gray-500 hover:text-gray-700'}`}>
                        轉譯
                    </NavLink>
                    <NavLink to="/practice" className={({ isActive }) => `px-4 py-2 rounded-md text-sm ${isActive ? 'bg-gray-200 text-gray-900 font-bold' : 'text-gray-500 hover:text-gray-700'}`}>
                        練習
                    </NavLink>
                </div>
                <button
                    className="p-2 text-gray-400 hover:text-gray-600 rounded-md hover:bg-gray-100"
                    onClick={() => setShowSettings(true)}
                    title="設定"
                >
                    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth="2">
                        <path strokeLinecap="round" strokeLinejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                        <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                    </svg>
                </button>
            </nav>
            <main className="p-6">
                <Routes>
                    <Route path="/" element={<Download />} />
                    <Route path="/transcribe" element={<Transcribe />} />
                    <Route path="/practice" element={<Practice />} />
                </Routes>
            </main>
            {showSettings && <Settings onClose={() => setShowSettings(false)} />}
        </BrowserRouter>
    )
}

export default App
