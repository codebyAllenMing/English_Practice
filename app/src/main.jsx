import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { LoadingProvider } from './Hooks/useLoading'
import { TranscribeProvider } from './Hooks/useTranscribe'
import './index.css'
import App from './App.jsx'

createRoot(document.getElementById('root')).render(
    <StrictMode>
        <LoadingProvider>
            <TranscribeProvider>
                <App />
            </TranscribeProvider>
        </LoadingProvider>
    </StrictMode>,
)
