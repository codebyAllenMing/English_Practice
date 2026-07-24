// 刪除確認 dialog:name 為空字串時不顯示
function ConfirmDeleteDialog({ name, onCancel, onConfirm }) {
    if (!name) return null
    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30" onClick={onCancel}>
            <div className="bg-surface border border-edge rounded-xl shadow-lg w-[420px] p-6" onClick={(e) => e.stopPropagation()}>
                <h2 className="text-lg font-bold mb-3">刪除確認</h2>
                <p className="text-sm text-ink-soft mb-1 break-all">確定要刪除「{name}」?</p>
                <p className="text-xs text-ink-faint mb-6">整個資料夾(含音檔、逐字稿與校正紀錄)會被移除,無法復原。</p>
                <div className="flex items-center justify-end gap-3">
                    <button
                        className="px-4 py-2 text-sm text-ink-soft border border-edge-strong rounded-lg hover:bg-card"
                        onClick={onCancel}
                    >
                        取消
                    </button>
                    <button
                        className="px-4 py-2 text-sm bg-red-600 text-white rounded-lg hover:bg-red-500"
                        onClick={() => onConfirm(name)}
                    >
                        刪除
                    </button>
                </div>
            </div>
        </div>
    )
}

export default ConfirmDeleteDialog
