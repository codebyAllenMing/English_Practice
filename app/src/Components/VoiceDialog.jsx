import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'

// 與 Rust 端 native_tts.rs 的 VOICES 表一致
const VOICE_GROUPS = [
	{ label: '女聲', voices: ['af_heart', 'af_bella', 'af_nova', 'af_sky', 'af_nicole', 'af_aoede'] },
	{ label: '男聲', voices: ['am_adam', 'am_michael', 'am_echo', 'am_liam', 'am_eric', 'am_fenrir'] },
]

// 每集的講者 → 聲音手動指定;「自動」= 交給 AI 性別判定/輪替
function VoiceDialog({ folder, onClose }) {
	const [speakers, setSpeakers] = useState([])
	const [voices, setVoices] = useState({})
	const [saved, setSaved] = useState(false)

	useEffect(() => {
		if (!folder) return
		invoke('get_lines', { folder })
			.then((lines) => {
				const seen = []
				for (const line of lines) {
					const m = line.match(/^\[([^\]]+)\]:/)
					if (m && !seen.includes(m[1])) seen.push(m[1])
				}
				setSpeakers(seen)
			})
			.catch(console.error)
		invoke('get_voices', { folder }).then(setVoices).catch(console.error)
	}, [folder])

	if (!folder) return null

	const handleSave = async () => {
		try {
			await invoke('save_voices', { folder, voices })
			setSaved(true)
			setTimeout(onClose, 600)
		} catch (err) {
			console.error(err)
		}
	}

	const setVoice = (speaker, value) => {
		setVoices((v) => {
			const next = { ...v }
			if (value) next[speaker] = value
			else delete next[speaker]
			return next
		})
	}

	return (
		<div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30" onClick={onClose}>
			<div className="bg-surface border border-edge rounded-xl shadow-lg w-[440px] p-6" onClick={(e) => e.stopPropagation()}>
				<h2 className="text-lg font-bold mb-1">聲音設定</h2>
				<p className="text-xs text-ink-faint mb-4 break-all">{folder}</p>

				{speakers.length === 0 ? (
					<p className="text-sm text-ink-faint mb-4">讀不到講者(尚未轉譯?)</p>
				) : (
					<div className="space-y-3 mb-5">
						{speakers.map((sp) => (
							<div key={sp} className="flex items-center gap-3">
								<span className="text-sm font-medium w-28 shrink-0 truncate" title={sp}>{sp}</span>
								<select
									className="flex-1 px-3 py-1.5 border border-edge-strong rounded-lg outline-none text-sm bg-surface"
									value={voices[sp] || ''}
									onChange={(e) => setVoice(sp, e.target.value)}
								>
									<option value="">自動(AI 判性別/輪替)</option>
									{VOICE_GROUPS.map((g) => (
										<optgroup key={g.label} label={g.label}>
											{g.voices.map((v) => (
												<option key={v} value={v}>{v}</option>
											))}
										</optgroup>
									))}
								</select>
							</div>
						))}
					</div>
				)}

				<p className="text-xs text-ink-faint mb-4">下次進入「▶ 練習」時生效。</p>

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

export default VoiceDialog
