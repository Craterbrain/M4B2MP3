# 🎧 M4B2MP3

A high-performance, parallelized CLI tool written in Rust designed to split M4B audiobooks into individual MP3 files based on chapter metadata.

## ✨ Features

- **Blazing Fast**: Leverages [Rayon](https://github.com/rayon-rs/rayon) for parallel chapter extraction.
- **Batch Processing**: Supports multiple files and recursive directory scanning.
- **Smart Metadata**: Preserves chapter titles, artist, and album tags.
- **Cover Art**: Automatically extracts embedded cover art.
- **Customizable**: Control bitrate, thread count, and output organization.

## 🛠 Prerequisites

M4B2MP3 relies on the following external tools which must be installed and available in your system `PATH`:
- **FFmpeg**
- **FFprobe** (usually bundled with FFmpeg)

## 🚀 Installation

### Download
You can find the latest pre-compiled binaries here:
Download Latest Release

### Build from Source
Ensure you have Rust installed, then:

```bash
git clone https://github.com/your-username/m4b2mp3.git
cd m4b2mp3
cargo build --release
```
The binary will be located at `target/release/m4b2mp3`.

## 📖 Usage

```bash
m4b2mp3 [options] <input_path1> [input_path2]...
```

### Options
* `-o, --output <dir>`: Base directory for output folders. A subfolder is created for each book.
* `-r, --recursive`: Recursively scan input directories for `.m4b` files.
* `-j, --threads <n>`: Limit the number of parallel ffmpeg tasks.
* `-b, --bitrate <b>`: Set manual output bitrate (e.g., `128k`). Defaults to source bitrate.
* `-h, --help`: Display help information.

### Example
```bash
m4b2mp3 -r -o ./Library /path/to/my/audiobooks
```

## 🤖 Built With
This tool was built with the assistance of **Gemini**.