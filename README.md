**简体中文** | [English](README.en.md)

# Fluent MD Todo

一款轻量的桌面 Markdown 待办应用，基于 Tauri 2、Rust 与 Fluent MD Todo 原始界面构建。

## 功能

- 打开并编辑 Markdown 待办清单（`- [ ]` / `- [x]`），支持标题与嵌套任务。
- 在应用内做的修改会立即写回所选的 Markdown 文件。
- 定期重新读取文件；其他应用改动了文件时，视图会自动刷新。
- 搜索、任务筛选、标题筛选、优先级、标签、行内编辑、删除，以及清除已完成任务。
- Markdown 原文编辑、Markdown 导出、主题、不透明度与紧凑视图控制。
- 可调整大小的原生桌面窗口与原生文件对话框。

## 在 Windows 上构建

安装 Node.js、pnpm、Rust 1.90 或更新版本、Microsoft C++ Build Tools 与 WebView2 Runtime，然后运行：

```powershell
pnpm install
pnpm tauri build
```

Windows 安装包会输出到 `src-tauri/target/release/bundle/`（NSIS/MSI，取决于本机安装的 Windows 工具链）。只构建可执行文件、不打安装包时运行 `pnpm tauri build --no-bundle`，可执行文件为 `src-tauri/target/release/fluent-md-todo.exe`。

## 在 macOS 上构建

安装 Node.js、pnpm、Rust 1.90 或更新版本与 Xcode Command Line Tools，然后运行：

```sh
pnpm install
pnpm tauri build
```

macOS 的应用包与磁盘镜像会输出到 `src-tauri/target/release/bundle/`。正式发布的签名与公证需要发布者的 Apple Developer 凭据。macOS 应用请在 macOS 上构建；Windows 可执行文件无法转换成 macOS 应用。

## 说明

- Markdown 始终保存在用户选择的文件里；应用不会把它导入单独的数据库。
- 本项目目前面向 Windows 与 macOS 桌面端，本版本不考虑移动端构建。
- Windows 上使用系统已安装的 Microsoft Edge WebView2 Runtime。
