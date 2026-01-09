use anyhow::{Context, Result};
use clap::Parser;
use rpassword;
use serde::Deserialize;
use ssh2::Session;
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // Remote host
    #[arg(short = 'o', long)]
    remote_host: Option<String>,

    // SSH username
    #[arg(short = 'u', long)]
    remote_user: Option<String>,

    // remote path on the Pi
    #[arg(short = 'r', long)]
    remote_bookmark_path: Option<PathBuf>,

    // local path on Windows
    #[arg(short = 'w', long)]
    local_windows_bookmark_path: Option<PathBuf>,

    // local path on Linux
    #[arg(short = 'l', long)]
    local_linux_bookmark_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    #[serde(default)]
    remote_host: Option<String>,

    #[serde(default)]
    remote_user: Option<String>,

    #[serde(default)]
    remote_bookmark_path: Option<PathBuf>,

    #[serde(default)]
    local_windows_bookmark_path: Option<PathBuf>,

    #[serde(default)]
    local_linux_bookmark_path: Option<PathBuf>,
}

fn load_file_config() -> Option<FileConfig> {
    let path = std::path::Path::new("config.toml");
    let content = std::fs::read_to_string(path).ok()?;
    Some(toml::from_str(&content).unwrap_or_else(|e| panic!("Invalid config.toml: {e}")))
}

struct FinalAppConfig {
    remote_host: String,
    remote_user: String,
    remote_bookmark_path: PathBuf,
    local_windows_bookmark_path: PathBuf,
    local_linux_bookmark_path: PathBuf,
}

fn build_config(args: Args, file: Option<FileConfig>) -> FinalAppConfig {
    let rh = args
        .remote_host
        .or_else(|| file.as_ref().and_then(|f| f.remote_host.clone()))
        .unwrap_or_else(|| panic!("ssh host must be provided via arg or config.toml!"));
    let ru = args
        .remote_user
        .or_else(|| file.as_ref().and_then(|f| f.remote_user.clone()))
        .unwrap_or_else(|| panic!("ssh user must be provided via arg or config.toml!"));
    let rbp = args
        .remote_bookmark_path
        .or_else(|| file.as_ref().and_then(|f| f.remote_bookmark_path.clone()))
        .unwrap_or_else(|| panic!("remote bookmark path must be provided via arg or config.toml!"));
    let lwbp = args
        .local_windows_bookmark_path
        .or_else(|| {
            file.as_ref()
                .and_then(|f| f.local_windows_bookmark_path.clone())
        })
        .unwrap_or_else(|| {
            panic!("windows bookmark path must be provided via arg or config.toml!")
        });
    let llbp = args
        .local_linux_bookmark_path
        .or_else(|| {
            file.as_ref()
                .and_then(|f| f.local_linux_bookmark_path.clone())
        })
        .unwrap_or_else(|| panic!("linux bookmark path must be provided via arg or config.toml!"));

    FinalAppConfig {
        remote_host: rh,
        remote_user: ru,
        remote_bookmark_path: rbp,
        local_windows_bookmark_path: lwbp,
        local_linux_bookmark_path: llbp,
    }
}

fn get_brave_bookmarks_path(windows_path: PathBuf, linux_path: PathBuf) -> Result<PathBuf> {
    // get config dir (or AppData in windows) based on OS platform
    let mut path = dirs::config_dir().context("Could not find config directory.")?;

    if cfg!(target_os = "windows") {
        path = dirs::data_local_dir().context("Could not find Local AppData")?;
        path.push(windows_path);
    } else {
        path.push(linux_path);
    }

    Ok(path)
}

fn main() -> Result<()> {
    // 1. Parse CLI arguments
    let args = Args::parse();

    // 2. Parse config.toml
    let file_cfg = load_file_config();

    // 3. Build config
    let config = build_config(args, file_cfg);

    // 2. Ask for password (securely)
    print!("Enter SSH password for {}: ", config.remote_user);
    io::stdout().flush()?;
    let password = rpassword::read_password()?;

    // 3. Local file information
    let local_path = get_brave_bookmarks_path(
        config.local_windows_bookmark_path,
        config.local_linux_bookmark_path,
    )?;
    let local_metadata =
        fs::metadata(&local_path).context("Could not find local Brave bookmarks file")?;
    let local_mtime = local_metadata.modified()?;
    let local_secs = local_mtime.duration_since(std::time::UNIX_EPOCH)?.as_secs();

    // 4. SSH connection
    println!("\nConnecting to {}...", config.remote_host);
    let tcp = TcpStream::connect(format!("{}:22", config.remote_host))
        .context("Failed to connect to the Pi via network.")?;
    let mut session = Session::new().context("Failed to create SSH session")?;
    session.set_tcp_stream(tcp);
    session.handshake().context("Failed to create handshake.")?;
    session
        .userauth_password(&config.remote_user, &password)
        .context("SSH Authentication failed to remote host")?;

    // 5. SFTP
    let sftp = session.sftp().context("Failed to open SFTP channel")?;
    let remote_path = Path::new(&config.remote_bookmark_path);

    // 6. Remote file information
    let remote_stat = sftp.stat(remote_path);

    match remote_stat {
        Ok(stat) => {
            let remote_secs = stat.mtime.unwrap_or(0);

            if local_secs > remote_secs {
                // Local is newer
                println!("Local bookmarks are newer. Uploading to Pi...");
                let mut remote_file = sftp.create(remote_path)?;
                let local_data = fs::read(&local_path)?;
                remote_file
                    .write_all(&local_data)
                    .context("Failed to write local data to remote host.")?;
                println!("Upload successful.");
            } else if remote_secs > local_secs {
                // Pi is newer
                println!("Pi has newer bookmarks. Updating local file...");
                let mut remote_file = sftp.open(remote_path)?;
                let mut buffer = Vec::new();
                remote_file.read_to_end(&mut buffer)?;
                fs::write(&local_path, buffer)?;
                println!("Local file updated from Pi.");
            } else {
                println!("Locations are in perfect sync.");
            }
        }
        Err(_) => {
            // Remote file doesn't exist yet -> CREATE
            println!("Remote file not found. Initial upload...");
            let mut remote_file = sftp.create(remote_path)?;
            let local_data = fs::read(&local_path)?;
            remote_file.write_all(&local_data)?;
            println!("Initial upload complete.");
        }
    };

    Ok(())
}
