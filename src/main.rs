use anyhow::{Context, Result};
use std::fs;
use std::os::unix::fs::chroot;
use tempfile::TempDir;
use std::path::Path;
use nix::sys::stat::{mknod, Mode, SFlag};

// Usage: your_docker.sh run <image> <command> <arg1> <arg2> ...
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    
    // file system isolation
    let tmp_dir = TempDir::new()?;
    let bin_dir = tmp_dir.path().join("bin");
    let dev_dir = tmp_dir.path().join("dev");

    fs::create_dir_all(&bin_dir)?;
    fs::create_dir_all(&dev_dir)?;

    let source_path_echo = Path::new("/bin/echo");  // Replace with the actual source path
    let source_path_null = Path::new("/dev/null");  // Replace with the actual source path

    let target_path_echo = bin_dir.join("echo");
    let target_path_null = dev_dir.join("null");

    fs::copy(source_path_echo, target_path_echo)?;
    // dev/null is a special type of node
    let dev_null_mode = Mode::from_bits_truncate(0o666); // Permissions: rw-rw-rw-
    mknod(&target_path_null, SFlag::S_IFCHR, dev_null_mode, nix::sys::stat::makedev(1, 3))
        .expect("Failed to create /dev/null");

    println!("Temporary directory created at: {:?}", tmp_dir.path());
    //chroot
    chroot("/")?;
    //chroot(tmp_dir.path())?;
    std::env::set_current_dir("/")?;

    

    let command = &args[3];
    let command_args = &args[4..];
    let output = std::process::Command::new(command)
        .args(command_args)
        .output()
        .with_context(|| {
            format!(
                "Tried to run '{}' with arguments {:?}",
                command, command_args
            )
        })?;
    unsafe { libc::unshare(libc::CLONE_NEWPID) };
    if output.status.success() {
        let std_out = std::str::from_utf8(&output.stdout)?;
        print!("{}", std_out);
	let std_err = std::str::from_utf8(&output.stderr)?;
	eprint!("{}", std_err);
    } else {
        let exit_code = output.status.code().unwrap_or(-1);
	if exit_code==-1 {
		std::process::exit(0);
	} else {
		println!("exit with code {}",  exit_code);
		std::process::exit(exit_code);
	}
    }

    Ok(())
}
