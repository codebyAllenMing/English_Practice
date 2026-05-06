import { createContext, useContext, useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

const TranscribeContext = createContext()

export function TranscribeProvider({ children }) {
    const [processing, setProcessing] = useState(false)
    const [progress, setProgress] = useState('')
    const [folder, setFolder] = useState('')
    const [result, setResult] = useState('')
    const [error, setError] = useState('')

    useEffect(() => {
        const unlisten = listen('transcribe-progress', (e) => {
            setProgress(e.payload)
        })
        return () => { unlisten.then(fn => fn()) }
    }, [])

    const startTranscribe = async (folderName) => {
        setProcessing(true)
        setFolder(folderName)
        setProgress('準備中...')
        setResult('')
        setError('')
        try {
            const res = await invoke('transcribe_audio', { folder: folderName })
            setResult(res)
        } catch (err) {
            setError(String(err))
        } finally {
            setProcessing(false)
            setProgress('')
            setFolder('')
        }
    }

    return (
        <TranscribeContext.Provider value={{ processing, progress, folder, result, error, startTranscribe, clearResult: () => { setResult(''); setError('') } }}>
            {children}
        </TranscribeContext.Provider>
    )
}

// eslint-disable-next-line react-refresh/only-export-components
export function useTranscribe() {
    return useContext(TranscribeContext)
}
