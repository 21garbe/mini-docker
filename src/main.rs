use anyhow::{Context, Result, anyhow};
use std::fs::{copy, create_dir_all};
use std::os::unix::fs::chroot;
use tempfile::TempDir;
use std::path::Path;
use std::env;


// Usage: your_docker.sh run <image> <command> <arg1> <arg2> ...
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    // let image = &args[2]; 

    // here must create a temp dir and put the command in it
    // then call chroot to this temp dir and then execute the 
    // command as done bellow  

    let command = &args[3];
    let command_args = &args[4..];


	let tmp_dir = TempDir::new()?;
	create_dir_all(tmp_dir.path().join("dev/null"))
		.context("failed in creating null device")?;
	let dst = tmp_dir
		.path()
		.join(command.split("/").last().unwrap());

	println!("the new location of the command is \n:{}", dst.to_string_lossy().to_string());

	let resolved = resolve_name(command).context("failed to resolving name of command")?;
	copy(resolved, dst).context("failed to copy")?;	

	chroot(tmp_dir.path()).context("failed to chroot")?;
	env::set_current_dir("/").context("failed to set cur dir to /")?;	

//	unsafe {libc::unshare(libc::CLONE_NEWPID)}

    let output = std::process::Command::new(command.split("/").last().unwrap())
        .args(command_args)
        .output()
        .with_context(|| {
            format!(
                "Tried to run '{}' with arguments {:?}",
                command, command_args
            )
        })?;
    
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

fn resolve_name(command: &str) -> Result<String> {
	println!("command name is {}", command.to_string());
    if Path::new(command).is_absolute() || command.contains('/') {
		if Path::new(command).exists() {
			return Ok(command.to_string());
		} else {
			return Err(anyhow!("doesn't exist"));
		} 
	} else if let Ok(paths) = env::var("PATH") {
		for path in paths.split(':') {
			let full_path = Path::new(path).join(command);
			if full_path.exists() && full_path.is_file() {
				return Ok(full_path.to_string_lossy().to_string());
			}
		}
	} 
	println!("error in resolving the name of the command"); 
	return Err(anyhow!("error resolving path"));
}

// fn main() -> Result<()> {
// 	let a = resolve_name("ls")?;
// 	println!("the resolved name is: {}", a);
// 	Ok(())
// }
