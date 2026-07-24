// 主題偏好:'system' | 'light' | 'dark',存 localStorage(純 UI 偏好,不進 config.json)
export const getThemePref = () => localStorage.getItem('theme') || 'system'

export const applyTheme = () => {
	const pref = getThemePref()
	const dark = pref === 'dark' || (pref === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
	document.documentElement.classList.toggle('dark', dark)
}

export const setThemePref = (pref) => {
	localStorage.setItem('theme', pref)
	applyTheme()
}

// 跟隨系統模式下,監聽系統明暗變化
export const watchSystemTheme = () => {
	const mq = window.matchMedia('(prefers-color-scheme: dark)')
	const fn = () => applyTheme()
	mq.addEventListener('change', fn)
	return () => mq.removeEventListener('change', fn)
}
