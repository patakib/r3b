use anyhow::{Context, Result};
use clap::Parser;
use rpassword;
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
    host: String,

    // SSH username
    #[arg(short, long, default_value = "tuxmeister")]
    user: String,

    // remote path on the Pi
    #[arg(short, long, default_value = "/home/tuxmeister/bookmarks.json")]
    remote_path: String,
}

fn get_brave_bookmarks_path() -> Result<PathBuf> {
    // get config dir (or AppData in windows) based on OS platform
    let mut path = dirs::config_dir().context("Could not find config directory.")?;

    if cfg!(target_os = "windows") {
        path = dirs::data_local_dir().context("Could not find Local AppData")?;
        path.push("BraveSoftware/Brave-Browser/User Data/Default/Bookmarks");
    } else {
        path.push("BraveSoftware/Brave-Browser/Default/Bookmarks");
    }

    Ok(path)
}

fn main() -> Result<()> {
    // 1. Parse CLI arguments
    let args = Args::parse();

    // 2. Ask for password (securely)
    print!("Enter SSH password for {}: ", args.user);
    io::stdout().flush()?;
    let password = rpassword::read_password()?;

    // 3. Local file information
    let local_path = get_brave_bookmarks_path()?;
    let local_metadata =
        fs::metadata(&local_path).context("Could not find local Brave bookmarks file")?;
    let local_mtime = local_metadata.modified()?;
    let local_secs = local_mtime.duration_since(std::time::UNIX_EPOCH)?.as_secs();

    // 4. SSH connection
    println!("\nConnecting to {}...", args.host);
    let tcp = TcpStream::connect(format!("{}:22", args.host))
        .context("Failed to connect to the Pi via network.")?;
    let mut session = Session::new().context("Failed to create SSH session")?;
    session.set_tcp_stream(tcp);
    session.handshake().context("Failed to create handshake.")?;
    session
        .userauth_password(&args.user, &password)
        .context("SSH Authentication failed to remote host")?;

    // 5. SFTP
    let sftp = session.sftp().context("Failed to open SFTP channel")?;
    let remote_path = Path::new(&args.remote_path);

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
