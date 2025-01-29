
use flate2::read::GzDecoder;
use nix::sys::stat::Mode;
use serde_json::Value;
use tar::Archive;
use std::env;
use std::fs::create_dir_all;
use std::path::Path;
use nix::sys::stat::{mknod, SFlag};
use anyhow::{Context, Ok, Result};
use std::os::unix::fs::chroot;


pub fn execute_command_with_args(command: &str, command_args: &[String]) -> Result<()> {
    // Unshare first, check for errors
    let unshare_result = unsafe { libc::unshare(libc::CLONE_NEWPID) };
    if unshare_result != 0 {
        return Err(anyhow::anyhow!("Failed to unshare PID namespace"));
    }

    let output = std::process::Command::new(command)
        .args(command_args)
        .output()
        .with_context(|| format!("Tried to run '{}' with arguments {:?}", command, command_args))?;

    if output.status.success() {
        print!("{}", std::str::from_utf8(&output.stdout)?);
        eprint!("{}", std::str::from_utf8(&output.stderr)?);
    } else {
        let exit_code = output.status.code().unwrap_or(-1);
        return Err(anyhow::anyhow!("Command exited with code {}", exit_code));
    }

    Ok(())
}


pub fn chroot_dir(root_dir: String) -> Result <()>{
    chroot(root_dir.clone()).context(format!("failed to chroot in {}", root_dir))?;
    std::env::set_current_dir("/").context("failed to chdir")?;
    println!("Chroot successful! Current directory: {:?}", env::current_dir());
    Ok(())
}

fn get_current_architecture_for_docker() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("amd64"),
        "aarch64" => Some("arm64v8"),
        "arm" => Some("arm32v7"), // Default to armv7 for 32-bit ARM
        "i686" | "i386" => Some("386"),
        "ppc64" | "ppc64le" => Some("ppc64le"),
        "riscv64" => Some("riscv64"),
        "s390x" => Some("s390x"),
        _ => None, // Unknown or unsupported architecture
    }
}

fn create_dev_null(root_dir: &str) -> Result<(), anyhow::Error> {
    let target_path_null = Path::new(root_dir).join("dev/null");
    if target_path_null.exists() {
        println!("/dev/null already exists at: {}", target_path_null.display());
    } else {
        let dev_null_mode = Mode::from_bits_truncate(0o666); // Permissions: rw-rw-rw-
        mknod(&target_path_null, SFlag::S_IFCHR, dev_null_mode, nix::sys::stat::makedev(1, 3))
        .expect("Failed to create /dev/null");
        println!("Created /dev/null at: {}", target_path_null.display());
    }
    Ok(())
}

pub fn pull(image: String, root_dir: String) -> Result<(), anyhow::Error> {
    let split : Vec<&str> = image.split(':').collect();
    let image_name = split[0];
    let mut version = "latest";
    if split.len() > 1 {
        version = split[1];
    }
    
    let auth = get_auth(String::from(image_name))?; // get token for pulled image
    // get image_manifest
    let blob = get_blob(String::from(image_name), String::from(version), auth.clone()).unwrap();
    blob_to_file(image_name.to_string(), blob, auth, root_dir.clone()).unwrap();
    let _ = create_dev_null(&root_dir); //ignore std::io error
    Ok(())
}
fn get_auth(image: String) -> Result<String, anyhow::Error> {
    let body = reqwest::blocking::get(
        &format!("https://auth.docker.io/token?service=registry.docker.io&scope=repository:library/{}:pull", image)
        )?
        .json::<serde_json::Value>()?;
    Ok(String::from(body["token"].as_str().unwrap()))
}

fn get_manifest_digest(image: &String, tag: &String, token: &String) -> Result<String>{
    let client = reqwest::blocking::Client::new();

    let response = client
        .get(&format!(
            "https://registry.hub.docker.com/v2/library/{}/manifests/{}",
            image, tag
        ))
        .bearer_auth(&token)
        .send()
        .context("Failed to send HTTP request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().unwrap_or_else(|_| "Failed to read response body".into());
            return Err(anyhow::anyhow!(
                "HTTP request failed with status {}: {}",
                status, body_text
            ));
        }
    
        let body_text = response.text().context("Failed to read response body")?;
    
        let json_manifest: Value =
            serde_json::from_str(&body_text).context("Failed to parse JSON response")?;
    
        match get_digest_for_architecture(&json_manifest) {
            std::result::Result::Ok(digest) => {
                println!("Manifest digest found : {}", digest);
                Ok(digest)
            }
            Err(e) => {
                eprintln!(
                    "Failed to extract digest. HTTP response body: {}",
                    body_text
                );
                Err(e).context("Failed to extract digest from JSON response")
            }
        }
}

fn get_blob(image: String, tag: String, token: String) -> Result<Vec<String>> {
    let digest= get_manifest_digest(&image, &tag, &token)?;
    let manifest = get_manifest_from_digest(image.as_str(), digest.as_str(), token.as_str())?;
    let mut res: Vec<String> = Vec::new();

    if let Some(fs_layers) = manifest["layers"].as_array() {
        for elem in fs_layers {
            if let Some(digest) = elem["digest"].as_str() {
                res.push(String::from(digest));
            }
        }
    }
    println!("Number of extracted layers: {}", res.len());
    Ok(res)
}

fn blob_to_file(image: String, blob: Vec<String>, token: String, dir: String) -> Result<()> {
    let client = reqwest::blocking::Client::new();
    for elem in blob {
        println!("Layer digest found : {}", elem);
        let body = &client
            .get(&format!("https://registry.hub.docker.com/v2/library/{}/blobs/{}", image, elem))
            .bearer_auth(token.clone()).send().unwrap().bytes().unwrap().to_vec();
        println!("Blob downloaded: {}", elem);
        if let Err(e) = unpack_tar_gz(body, dir.as_str()) {
            println!("Error unpacking tar file: {}", e);
        }
    }
    Ok(())
}

fn get_digest_for_architecture(manifest: &serde_json::Value) -> Result<String> {
    
    // Match runtime architecture to Docker API key
    let docker_arch_key = match get_current_architecture_for_docker() {
        Some(key) => key,
        None => return Err(anyhow::anyhow!("Unsupported architecture")),
    };

    // Parse the manifest to find the correct digest
    if let Some(manifests) = manifest.get("manifests").and_then(|m| m.as_array()) {
        for entry in manifests {
            if let Some(platform) = entry.get("platform") {
                if platform.get("architecture").and_then(|a| a.as_str()) == Some(docker_arch_key) {
                    if let Some(digest) = entry.get("digest").and_then(|d| d.as_str()) {
                        return Ok(digest.to_string());
                    }
                }
            }
        }
    }

    Err(anyhow::anyhow!("Digest for architecture '{}' not found", docker_arch_key))
}
fn get_manifest_from_digest(image: &str, digest: &str, token: &str) -> Result<serde_json::Value> {
    let client = reqwest::blocking::Client::new();

    let response = client
        .get(&format!(
            "https://registry.hub.docker.com/v2/library/{}/manifests/{}",
            image, digest
        ))
        .bearer_auth(token)
        .send()
        .context("Failed to send HTTP request for manifest")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to get manifest for digest. Status: {}",
            response.status()
        ));
    }

    let body_text = response.text().context("Failed to read response body")?;
    //println!("{}", body_text);
    let json_body: serde_json::Value = serde_json::from_str(&body_text)
        .context("Failed to parse JSON response")?;

    Ok(json_body)
}


fn unpack_tar_gz(data: &Vec<u8>, dest_dir: &str) -> Result<()> {
    let decoder = GzDecoder::new(&data[..]);
    let mut archive = Archive::new(decoder);

    create_dir_all(dest_dir)?;

    for entry in archive.entries()? {
        let mut entry = entry?;

        let file_path = entry.path()?;
        let dest_path = dest_dir.to_string() + "/" + &file_path.to_string_lossy();

        if file_path.is_dir() {
            create_dir_all(dest_path)?;
        } else {
            entry.unpack(dest_path)?;
        }
    }

    println!("Extracted tar blob into: {}", dest_dir);
    Ok(())
}

// fn update_path() {
//     // Get the current PATH
//     let current_path = env::var("PATH").unwrap_or_else(|_| String::new());

//     // Add /bin to the PATH if it's not already included
//     let new_path = if current_path.contains("/bin") {
//         current_path
//     } else {
//         format!("{}/bin:{}", env::current_dir().unwrap().display(), current_path)
//     };
//     println!("{}", new_path);

//     // Set the updated PATH
//     env::set_var("PATH", new_path);
// }
// fn check_ls_in_chroot() -> Result<()> {
//     // Try to open /bin/ls
//     let file_path = "/bin/ls";
//     match File::open(file_path) {
//         Ok(mut file) => {
//             println!("Successfully opened: {}", file_path);
            
//             // Read the first few bytes to ensure the file is valid
//             let mut buffer = [0u8; 10]; // Read first 10 bytes
//             file.read_exact(&mut buffer)?;
//             println!("First 10 bytes: {:?}", buffer);
//         }
//         Err(e) => {
//             println!("Failed to open {}: {}", file_path, e);
//         }
//     }

//     Ok(())
// }
