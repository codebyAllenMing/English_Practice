import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'

function Settings({ onClose }) {
    const [token, setToken] = useState('')
    const [saved, setSaved] = useState(false)

    useEffect(() => {
        invoke('get_config').then((config) => {
            setToken(config.hf_token || '')
        })
    }, [])

    const handleSave = async () => {
        try {
            const config = await invoke('get_config')
            await invoke('save_config', { config: { ...config, hf_token: token } })
            setSaved(true)
            setTimeout(() => setSaved(false), 2000)
        } catch (err) {
            console.error(err)
        }
    }

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30" onClick={onClose}>
            <div className="bg-white rounded-xl shadow-lg w-[480px] p-6" onClick={(e) => e.stopPropagation()}>
                <div className="flex items-center justify-between mb-6">
                    <h2 className="text-lg font-bold">設定</h2>
                    <button className="text-gray-400 hover:text-gray-600" onClick={onClose}>
                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth="2">
                            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                <label className="block text-sm font-medium text-gray-700 mb-2">HuggingFace Token</label>
                <input
                    type="password"
                    className="w-full px-4 py-2 border border-gray-300 rounded-lg outline-none focus:border-gray-500 text-sm mb-4"
                    placeholder="hf_..."
                    value={token}
                    onChange={(e) => setToken(e.target.value)}
                />

                <div className="flex items-center justify-end gap-3">
                    {saved && <span className="text-sm text-green-600">已儲存</span>}
                    <button
                        className="px-4 py-2 text-sm text-gray-600 border border-gray-300 rounded-lg hover:bg-gray-50"
                        onClick={onClose}
                    >
                        取消
                    </button>
                    <button
                        className="px-4 py-2 bg-gray-800 text-white text-sm rounded-lg hover:bg-gray-700"
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
