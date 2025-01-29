use anyhow::Result;

mod utils;

// Usage: your_docker.sh run <image> <docker_root_path> <command> <arg1> <arg2> ...
 fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let root_dir = &args[1];
    let image = &args[2];
    let command = &args[3];
    let command_args = &args[4..];

    utils::pull(image.clone(), root_dir.clone().to_string()).unwrap();
    
    utils::chroot_dir(root_dir.clone())?;

    println!("Executing command {} : \n", command);
    utils::execute_command_with_args(command, command_args)?;

    Ok(())
}
