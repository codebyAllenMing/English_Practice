import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getThemePref, setThemePref } from '../theme'

const THEME_OPTIONS = [
    { value: 'system', label: '跟隨系統' },
    { value: 'light', label: '亮色' },
    { value: 'dark', label: '暗色' },
]

// 密碼欄位:右側眼睛可切換顯示原始值
function SecretInput({ value, onChange, placeholder }) {
    const [show, setShow] = useState(false)
    return (
        <div className="relative mb-4">
            <input
                type={show ? 'text' : 'password'}
                className="w-full px-4 py-2 pr-10 border border-edge-strong rounded-lg outline-none focus:border-ink-faint text-sm"
                placeholder={placeholder}
                value={value}
                onChange={onChange}
            />
            <button
                type="button"
                className="absolute right-3 top-1/2 -translate-y-1/2 text-ink-faint hover:text-ink-soft"
                onClick={() => setShow((s) => !s)}
                title={show ? '隱藏' : '顯示'}
            >
                {show ? (
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth="2">
                        <path strokeLinecap="round" strokeLinejoin="round" d="M3.98 8.223A10.477 10.477 0 001.934 12C3.226 16.338 7.244 19.5 12 19.5c.993 0 1.953-.138 2.863-.395M6.228 6.228A10.45 10.45 0 0112 4.5c4.756 0 8.773 3.162 10.065 7.498a10.523 10.523 0 01-4.293 5.774M6.228 6.228L3 3m3.228 3.228l3.65 3.65m7.894 7.894L21 21m-3.228-3.228l-3.65-3.65m0 0a3 3 0 10-4.243-4.243m4.242 4.242L9.88 9.88" />
                    </svg>
                ) : (
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth="2">
                        <path strokeLinecap="round" strokeLinejoin="round" d="M2.036 12.322a1.012 1.012 0 010-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178z" />
                        <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                    </svg>
                )}
            </button>
        </div>
    )
}

// section:未指定 = 完整設定;"transcribe" = 只顯示轉譯相關;"correct" = 只顯示校正相關
function Settings({ onClose, section }) {
    const showTranscribe = !section || section === 'transcribe'
    const showCorrect = !section || section === 'correct'
    const title = section === 'transcribe' ? '轉譯設定' : section === 'correct' ? '校正設定' : '設定'
    const [token, setToken] = useState('')
    const [apiKey, setApiKey] = useState('')
    const [correctionMode, setCorrectionMode] = useState('api')
    const [themePref, setThemePrefState] = useState(getThemePref())
    const [saved, setSaved] = useState(false)

    // 外觀立即生效,不用按儲存
    const handleTheme = (value) => {
        setThemePref(value)
        setThemePrefState(value)
    }

    useEffect(() => {
        invoke('get_config').then((config) => {
            setToken(config.hf_token || '')
            setApiKey(config.anthropic_api_key || '')
            setCorrectionMode(config.correction_mode || 'api')
        })
    }, [])

    const handleSave = async () => {
        try {
            const config = await invoke('get_config')
            await invoke('save_config', {
                config: { ...config, hf_token: token, anthropic_api_key: apiKey, correction_mode: correctionMode },
            })
            // 通知其他頁面(如校正頁的模式徽章)重讀 config
            window.dispatchEvent(new Event('config-saved'))
            setSaved(true)
            setTimeout(() => setSaved(false), 2000)
        } catch (err) {
            console.error(err)
        }
    }

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30" onClick={onClose}>
            <div className="bg-surface border border-edge rounded-xl shadow-lg w-[480px] p-6" onClick={(e) => e.stopPropagation()}>
                <div className="flex items-center justify-between mb-6">
                    <h2 className="text-lg font-bold">{title}</h2>
                    <button className="text-ink-faint hover:text-ink-soft" onClick={onClose}>
                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth="2">
                            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                {!section && (
                    <>
                        <label className="block text-sm font-medium text-ink-soft mb-2">外觀</label>
                        <div className="inline-flex rounded-lg border border-edge-strong overflow-hidden mb-4">
                            {THEME_OPTIONS.map((opt) => (
                                <button
                                    key={opt.value}
                                    className={`px-3 py-1.5 text-sm ${themePref === opt.value ? 'bg-primary text-white' : 'text-ink-soft hover:bg-card'}`}
                                    onClick={() => handleTheme(opt.value)}
                                >
                                    {opt.label}
                                </button>
                            ))}
                        </div>
                    </>
                )}

                {showTranscribe && (
                    <>
                        <label className="block text-sm font-medium text-ink-soft mb-2">HuggingFace Token</label>
                        <SecretInput
                            placeholder="hf_..."
                            value={token}
                            onChange={(e) => setToken(e.target.value)}
                        />
                    </>
                )}

                {showCorrect && (
                    <>
                <label className="block text-sm font-medium text-ink-soft mb-2">AI 校正模式</label>
                <div className="flex items-center gap-3 mb-2">
                    <span
                        className={`text-sm cursor-pointer select-none ${correctionMode === 'cli' ? 'text-ink font-medium' : 'text-ink-faint'}`}
                        onClick={() => setCorrectionMode('cli')}
                    >
                        本機 Claude CLI
                    </span>
                    <button
                        type="button"
                        role="switch"
                        aria-checked={correctionMode === 'api'}
                        className={`relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200 ${correctionMode === 'api' ? 'bg-purple-600' : 'bg-muted-strong'}`}
                        onClick={() => setCorrectionMode(correctionMode === 'api' ? 'cli' : 'api')}
                    >
                        <span
                            className={`inline-block h-5 w-5 transform rounded-full bg-white shadow transition-transform duration-200 ${correctionMode === 'api' ? 'translate-x-[22px]' : 'translate-x-0.5'}`}
                        />
                    </button>
                    <span
                        className={`text-sm cursor-pointer select-none ${correctionMode === 'api' ? 'text-ink font-medium' : 'text-ink-faint'}`}
                        onClick={() => setCorrectionMode('api')}
                    >
                        Anthropic API
                    </span>
                </div>
                <p className="text-xs text-ink-faint mb-4">
                    {correctionMode === 'cli'
                        ? '用這台電腦登入的 Claude 帳號,吃訂閱額度,不需 API Key'
                        : '用 API Key 按量計費(單次校正約 0.01 美金內)'}
                </p>

                {correctionMode === 'api' && (
                    <>
                        <label className="block text-sm font-medium text-ink-soft mb-2">Anthropic API Key</label>
                        <SecretInput
                            placeholder="sk-ant-..."
                            value={apiKey}
                            onChange={(e) => setApiKey(e.target.value)}
                        />
                    </>
                )}
                    </>
                )}

                <div className="flex items-center justify-end gap-3">
                    {saved && <span className="text-sm text-green-600 dark:text-green-400">已儲存</span>}
                    <button
                        className="px-4 py-2 text-sm text-ink-soft border border-edge-strong rounded-lg hover:bg-card"
                        onClick={onClose}
                    >
                        取消
                    </button>
                    <button
                        className="px-4 py-2 bg-primary text-white text-sm rounded-lg hover:bg-primary-hover"
                        onClick={handleSave}
                    >
                        儲存
                    </button>
                </div>
            </div>
        </div>
    )
}

export default Settings
