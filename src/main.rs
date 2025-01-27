use anyhow::{Context, Result};
use std::fs;
use std::os::unix::fs::chroot;
use tempfile::TempDir;


// Usage: your_docker.sh run <image> <command> <arg1> <arg2> ...
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    // let image = &args[2]; 

    // here must create a temp dir and put the command in it
    // then call chroot to this temp dir and then execute the 
    // command as done bellow  
    

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
