use serde::Deserialize;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use regex::Regex;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Represents the top-level structure of the ffprobe JSON output for chapters.
#[derive(Deserialize, Debug)]
struct ProbeResult {
    chapters: Vec<Chapter>,
    format: Option<FormatInfo>,
}

/// Represents the global format information of the file.
#[derive(Deserialize, Debug)]
struct FormatInfo {
    tags: Option<GlobalTags>,
    bit_rate: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GlobalTags {
    #[serde(alias = "ARTIST", alias = "artist")]
    artist: Option<String>,
    #[serde(alias = "ALBUM", alias = "album")]
    album: Option<String>,
}

/// Represents a single chapter extracted from the media file.
#[derive(Deserialize, Debug)]
struct Chapter {
    /// Start time of the chapter in seconds (as a string from JSON).
    start_time: String,
    /// Optional end time of the chapter in seconds.
    end_time: Option<String>,
    /// Metadata tags associated with the chapter.
    tags: Option<Tags>,
}

/// Represents tags within a chapter, specifically looking for the title.
#[derive(Deserialize, Debug)]
struct Tags {
    /// The title of the chapter.
    title: Option<String>,
}

/// Checks if the required external binaries are available in the PATH.
fn check_dependencies() -> Result<(), String> {
    for cmd in &["ffmpeg", "ffprobe"] {
        if Command::new("which").arg(cmd).output().map(|o| !o.status.success()).unwrap_or(true) {
            return Err(format!("Error: '{}' is not installed or not in PATH.", cmd));
        }
    }
    Ok(())
}

/// Prints the help message including flag details and default values.
fn print_help(program: &str) {
    println!("m4b2mp3 - Audiobook M4B to MP3 chapter splitter");
    println!();
    println!("Usage: {} [options] <input_path1> [input_path2]...", program);
    println!();
    println!("Arguments:");
    println!("  <input_path>       M4B file(s) or directories to scan for M4B files.");
    println!();
    println!("Options:");
    println!("  -o, --output <dir> Base directory for output folders. Default: same as input file.");
    println!("  -r, --recursive    Recursive directory scanning.");
    println!("  -j, --threads <n>  Number of parallel ffmpeg tasks. Default: logical CPU cores.");
    println!("  -b, --bitrate <b>  Output audio bitrate (e.g. 128k). Default: source bitrate (fallback: 192k).");
    println!("  -h, --help         Display this help information.");
}

/// Recursively collects all .m4b files from a given path.
fn collect_m4b_files(path: &std::path::Path, recursive: bool, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        if path.extension().map_or(false, |ext| ext.to_ascii_lowercase() == "m4b") {
            files.push(path.to_path_buf());
        }
    } else if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if (p.is_dir() && recursive) || p.is_file() {
                collect_m4b_files(&p, recursive, files)?;
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Check for ffmpeg and ffprobe before doing anything.
    check_dependencies().map_err(|e| e)?;

    // 1. Basic command line argument handling.
    let args: Vec<String> = env::args().collect();
    
    if args.contains(&"-h".to_string()) || args.contains(&"--help".to_string()) {
        print_help(&args[0]);
        std::process::exit(0);
    }

    let mut input_paths = Vec::new();
    let mut thread_limit = 0;
    let mut manual_bitrate = None;
    let mut output_base_dir = None;
    let mut recursive = false;

    // Simple manual argument parsing to handle optional flags and positional paths.
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-j" | "--threads" => {
                if i + 1 < args.len() {
                    thread_limit = args[i + 1].parse().unwrap_or(0);
                    i += 2;
                } else {
                    eprintln!("Error: -j/--threads requires a numeric value.");
                    std::process::exit(1);
                }
            }
            "-b" | "--bitrate" => {
                if i + 1 < args.len() {
                    manual_bitrate = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: -b/--bitrate requires a value (e.g. 128k).");
                    std::process::exit(1);
                }
            }
            "-o" | "--output" => {
                if i + 1 < args.len() {
                    output_base_dir = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    eprintln!("Error: -o/--output requires a path.");
                    std::process::exit(1);
                }
            }
            "-r" | "--recursive" => {
                recursive = true;
                i += 1;
            }
            _ => {
                input_paths.push(&args[i]);
                i += 1;
            }
        }
    }

    if input_paths.is_empty() {
        eprintln!("Error: Missing input path(s). Run with --help for full usage.");
        std::process::exit(1);
    }

    let mut files_to_process = Vec::new();
    for p_str in input_paths {
        collect_m4b_files(&PathBuf::from(p_str), recursive, &mut files_to_process)?;
    }

    if files_to_process.is_empty() {
        print_help(&args[0]);
        std::process::exit(1);
    }

    println!("Found {} M4B files to process:", files_to_process.len());
    for f in &files_to_process {
        println!("  - {}", f.display());
    }
    println!();

    // Pre-compile regex for sanitizing titles for use as filenames.
    let re_sanitize = Regex::new(r"[/:]")?;
    let re_clean = Regex::new(r"[^a-zA-Z0-9 _-]")?;
    let re_collapse = Regex::new(r"\s+")?;

    // Initialize Rayon global thread pool if a limit was requested via flag.
    if thread_limit > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(thread_limit)
            .build_global()?;
    }

    for infile_path in files_to_process {
        let infile_str = infile_path.to_str().ok_or("Invalid input path encoding")?;
        let file_stem = infile_path.file_stem().ok_or("Invalid filename")?.to_string_lossy();

        // Determine output directory: base dir + file stem OR same dir as input file.
        let outdir = match &output_base_dir {
            Some(base) => base.join(file_stem.as_ref()),
            None => {
                let mut p = infile_path.clone();
                p.set_extension("");
                p
            }
        };

        if !outdir.exists() {
            fs::create_dir_all(&outdir)?;
        }

        println!("Processing: {}", infile_path.display());

    // 2. Extract chapters and global metadata as JSON via ffprobe.
    let output = Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-print_format", "json",
            "-show_chapters",
            "-show_format",
            "-i", infile_str,
        ])
        .output()?;

    if !output.status.success() {
        eprintln!("Error: ffprobe failed to read chapters for {:?}.", infile_path);
        continue;
    }

    let probe: ProbeResult = serde_json::from_slice(&output.stdout)?;
    let nch = probe.chapters.len();

    // Extract global metadata for tagging
    let artist = probe.format.as_ref().and_then(|f| f.tags.as_ref()).and_then(|t| t.artist.as_ref());
    let album = probe.format.as_ref().and_then(|f| f.tags.as_ref()).and_then(|t| t.album.as_ref());

    // Determine bitrate: manually provided or detected from original
    let final_bitrate = match &manual_bitrate {
        Some(b) => b.clone(),
        None => probe.format.as_ref()
            .and_then(|f| f.bit_rate.as_ref())
            .cloned()
            .unwrap_or_else(|| "192k".to_string()),
    };

    if nch == 0 {
        println!("No chapters found in {:?}.", infile_path);
        continue;
    }

    // Try to extract cover art (ignores error if no video stream/art is present)
    let cover_path = outdir.join("cover.jpg");
    let _ = Command::new("ffmpeg")
        .args(&[
            "-hide_banner", "-loglevel", "error", "-y",
            "-i", infile_str,
            "-map", "0:v", "-map", "-0:a", "-c", "copy",
            cover_path.to_str().unwrap_or("cover.jpg"),
        ])
        .status();

    // Initialize progress bar for parallel extraction
    let pb = ProgressBar::new(nch as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")?
        .progress_chars("#>-"));
    pb.set_message("Exporting chapters");

    // 3. Iterate chapters and export via ffmpeg in parallel.
    probe.chapters.par_iter().enumerate().try_for_each(|(i, ch)| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let idx = i + 1;
        let title_raw = ch.tags.as_ref()
            .and_then(|t| t.title.as_ref())
            .cloned()
            .unwrap_or_else(|| format!("Chapter{:03}", idx));

        // Clean up the chapter title to ensure it's a safe filename.
        let sanitized = re_sanitize.replace_all(&title_raw, "-");
        let cleaned = re_clean.replace_all(&sanitized, "");
        let collapsed = re_collapse.replace_all(&cleaned, " ");
        let safe_title = collapsed.trim();

        let out_filename = format!("{:03} - {}.mp3", idx, safe_title);
        let out_path = outdir.join(out_filename);
        let out_str = out_path.to_str().ok_or("Invalid output path encoding")?;

        pb.set_message(format!("Chapter {:03}: {}", idx, safe_title));

        let mut ffmpeg = Command::new("ffmpeg");
        // Global settings and input seeking (-ss and -to before -i) for speed.
        ffmpeg.args(&["-hide_banner", "-loglevel", "error", "-y", "-ss", &ch.start_time]);

        if let Some(ref end) = ch.end_time {
            if end.as_str() != "null" && !end.as_str().is_empty() {
                ffmpeg.args(&["-to", end]);
            }
        }

        // Input file and audio encoding parameters.
        ffmpeg.args(&["-i", infile_str, "-vn", "-acodec", "libmp3lame", "-b:a", &final_bitrate, out_str]);

        // Apply metadata tags.
        ffmpeg.arg("-metadata").arg(format!("title={}", title_raw));
        ffmpeg.arg("-metadata").arg(format!("track={}/{}", idx, nch));
        
        if let Some(a) = artist {
            ffmpeg.arg("-metadata").arg(format!("artist={}", a));
        }
        if let Some(al) = album {
            ffmpeg.arg("-metadata").arg(format!("album={}", al));
        }

        // Add a comment to identify the tool.
        ffmpeg.arg("-metadata").arg("comment=Split by m4b2mp3-rust");

        // Run the command and report failures.
        let status = ffmpeg.status()?;
        pb.inc(1);

        if !status.success() {
            pb.println(format!("Warning: Failed to export chapter {} ({})", idx, safe_title));
        }
        Ok(())
    })?;

    pb.finish_with_message(format!("Export complete for {}", file_stem));
    println!("Done. Exported {} chapters to: {:?}", nch, outdir);
    println!();
    }

    Ok(())
}
