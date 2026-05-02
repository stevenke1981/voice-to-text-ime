# Voice-to-Text IME 專案進度表

## ✅ 已完成 (Done)
- [x] **Phase 1: 核心引擎開發**
    - [x] 初始化 Rust 專案與 Git 倉庫
    - [x] 設定 Candle 框架並強制開啟 CUDA (GPU) 加速
    - [x] 整合 `hf-hub` 自動下載量化版 Whisper 模型 (Q4_0)
    - [x] 實作 `whisper.rs` 推論引擎，支援多國語言 (預設中文)
    - [x] 整合 `melfilters.bytes` 進行高品質頻譜轉換
- [x] **Phase 2: 音訊與輸入監聽**
    - [x] 實作 `audio.rs` 錄音功能，支援 16kHz 自動重取樣 (`rubato`)
    - [x] 實作 `src/main.rs` 全域快捷鍵監聽 (`rdev`)
    - [x] 支援「按住說話 (Hold-to-Talk)」模式
    - [x] 實作 `enigo` 自動文字輸入模擬
- [x] **Phase 3: 功能整合與修正**
    - [x] 修正模型下載 404 錯誤
    - [x] 優化按鍵監聽邏輯，支援 Alt/AltGr 觸發
- [x] **Phase 4: Windows GUI 介面開發**
    - [x] 選擇 GUI 框架 (使用 `egui` 搭配 `eframe`)
    - [x] 實作系統托盤 (System Tray) 功能
    - [x] 實作狀態浮窗 (錄音中/辨識中/辨識結果)

## 📅 待辦清單 (To-Do)
- [ ] **介面與體驗**
    - [ ] 設定頁面：自定義快捷鍵、切換模式 (Toggle/Hold)、選擇語言
    - [ ] 視覺化音量條 (Audio visualizer)
- [ ] **系統優化**
    - [ ] 開機自動啟動 (Start on boot)
    - [ ] 支援多種 Whisper 模型大小選擇 (Tiny/Base/Small)
    - [ ] 錯誤處理優化 (如 CUDA 驅動缺失時的提示)
- [ ] **打包與分發**
    - [ ] 製作 Windows 安裝包 (MSI 或 Setup.exe)
