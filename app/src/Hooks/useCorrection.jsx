import { createContext, useContext, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'

const CorrectionContext = createContext()

// 校正狀態放全域 Provider(與 useTranscribe 同模式):
// 切換分頁不會遺失「校正中」,完成時廣播 correction-done 讓在場頁面刷新清單
export function CorrectionProvider({ children }) {
	const [correcting, setCorrecting] = useState('')
	const [results, setResults] = useState({})
	const [lastMessage, setLastMessage] = useState('')

	const startCorrect = async (name) => {
		if (correcting) return
		setCorrecting(name)
		setLastMessage('')
		setResults((r) => ({ ...r, [name]: null }))
		try {
			const res = await invoke('correct_transcript', { folder: name })
			setResults((r) => ({ ...r, [name]: res }))
			setLastMessage(`校正完成:${name}(說話者 ${res.speakers} 位、修正 ${res.fixes} 行)`)
			// 校正完自動接跑分析講義(一次性,產物快取 analysis.json);失敗不擋流程,練習頁可手動重生成
			setLastMessage(`校正完成:${name},生成講義中…`)
			try {
				const a = await invoke('analyze_transcript', { folder: name })
				setLastMessage(`講義就緒:${name}(大綱 ${a.sections} 段、詞彙 ${a.vocab} 則)`)
			} catch (err) {
				setLastMessage(`${name} 校正完成,但講義生成失敗:${err}`)
			}
		} catch (err) {
			setResults((r) => ({ ...r, [name]: { error: String(err) } }))
			setLastMessage(`錯誤:${err}`)
		} finally {
			setCorrecting('')
			window.dispatchEvent(new Event('correction-done'))
		}
	}

	const reportResult = (name, res) => setResults((r) => ({ ...r, [name]: res }))

	const clearResult = (name) =>
		setResults((r) => {
			const next = { ...r }
			delete next[name]
			return next
		})

	return (
		<CorrectionContext.Provider value={{ correcting, results, lastMessage, startCorrect, reportResult, clearResult }}>
			{children}
		</CorrectionContext.Provider>
	)
}

// eslint-disable-next-line react-refresh/only-export-components
export function useCorrection() {
	return useContext(CorrectionContext)
}
