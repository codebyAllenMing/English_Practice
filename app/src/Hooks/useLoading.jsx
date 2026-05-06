import { createContext, useContext, useState } from 'react'

const LoadingContext = createContext()

export function LoadingProvider({ children }) {
    const [visible, setVisible] = useState(false)
    const [message, setMessage] = useState('')

    const loading = (show, msg = '載入中...') => {
        setVisible(show)
        setMessage(msg)
    }

    return (
        <LoadingContext.Provider value={loading}>
            {children}
            {visible && (
                <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
                    <div className="flex items-center gap-3 px-6 py-4 bg-white rounded-xl shadow-lg">
                        <svg className="animate-spin h-5 w-5 text-gray-600" viewBox="0 0 24 24" fill="none">
                            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                        </svg>
                        <span className="text-sm text-gray-700">{message}</span>
                    </div>
                </div>
            )}
        </LoadingContext.Provider>
    )
}

// eslint-disable-next-line react-refresh/only-export-components
export function useLoading() {
    return useContext(LoadingContext)
}
