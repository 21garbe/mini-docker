use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::fs::create_dir_all;
use std::fs::File;
use std::os::unix::fs::chroot;
use tempfile::TempDir;
use std::path::Path;
use nix::sys::stat::{mknod, Mode, SFlag};
use reqwest::Error;

use serde_json::json;
use serde_json::Value; // For JSON parsinguse std::fs::{File, create_dir_all};
use std::io::{self, Read};
use flate2::read::GzDecoder;
use tar::{Archive, Entry};
use tokio; // Needed for async runtime
use libflate::gzip::{Encoder, Decoder};


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
fn unpack_tar_gz(data: &Vec<u8>, dest_dir: &str) -> io::Result<()> {
    let decoder = GzDecoder::new(&data[..]);
    let mut archive = Archive::new(decoder);

    // Create the destination directory if it doesn't exist
    create_dir_all(dest_dir)?;

    // Loop through each entry in the tar archive
    for entry in archive.entries()? {
        let mut entry = entry?;

        let file_path = entry.path()?;
        let dest_path = dest_dir.to_string() + "/" + &file_path.to_string_lossy();

        if file_path.is_dir() {
            // If it's a directory, ensure it's created
            create_dir_all(dest_path)?;
        } else {
            // If it's a file, extract it
            entry.unpack(dest_path)?;
        }
    }

    println!("Extracted tar blob into: {}", dest_dir);
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

    // Send the GET request for the manifest using the digest
    let response = client
        .get(&format!(
            "https://registry.hub.docker.com/v2/library/{}/manifests/{}",
            image, digest
        ))
        .bearer_auth(token)
        .send()
        .context("Failed to send HTTP request for manifest")?;

    // Check if the response is successful
    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to get manifest for digest. Status: {}",
            response.status()
        ));
    }

    // Parse the response body to JSON
    let body_text = response.text().context("Failed to read response body")?;
    println!("{}", body_text);
    let json_body: serde_json::Value = serde_json::from_str(&body_text)
        .context("Failed to parse JSON response")?;

    Ok(json_body)
}
fn pull(image: String, root_dir: String) -> Result<(), reqwest::Error> {
    let split : Vec<&str> = image.split(':').collect();
    let image_name = split[0];
    let mut version = "latest";
    if split.len() > 1 {
        version = split[1];
    }
    
    let auth = get_auth(String::from(image_name))?;
    let blob = get_blob(String::from(image_name), String::from(version), auth.clone()).unwrap();
    blob_to_file(image_name.to_string(), blob, auth, root_dir).unwrap();
    Ok(())
}
fn get_auth(image: String) -> Result<String, reqwest::Error> {
    let body = reqwest::blocking::get(
        &format!("https://auth.docker.io/token?service=registry.docker.io&scope=repository:library/{}:pull", image)
        )?
        .json::<serde_json::Value>()?;
    Ok(String::from(body["token"].as_str().unwrap()))
}


fn get_blob(image: String, tag: String, token: String) -> Result<Vec<String>> {
    let client = reqwest::blocking::Client::new();

    // Send the request
    let body = client
        .get(&format!(
            "https://registry.hub.docker.com/v2/library/{}/manifests/{}",
            image, tag
        ))
        .bearer_auth(&token)
        .send()
        .context("Failed to send HTTP request")?; // Use `?` to propagate the error, now compatible with `anyhow::Result`

    // Print the raw response body for debugging purposes
    let body_text = body.text().context("Failed to read response body")?;
    //println!("Response Body: {}", body_text);

    // Parse the JSON response
    let json_body: serde_json::Value = serde_json::from_str(&body_text)
        .context("Failed to parse JSON response")?;
    //println!("JSON response {}", json_body);
    println!("ohoh");
    let digest = get_digest_for_architecture(&json_body)?;
    println!("{}", digest);
    let manifest = get_manifest_from_digest(image.as_str(), digest.as_str(), token.as_str())?;
    println!("{}", manifest);
    let mut res: Vec<String> = Vec::new();

    if let Some(fs_layers) = manifest["layers"].as_array() {
        for elem in fs_layers {
            if let Some(digest) = elem["digest"].as_str() {
                res.push(String::from(digest));
            }
        }
    }
    println!("Length of res: {}", res.len());
    Ok(res)
}

fn blob_to_file(image: String, blob: Vec<String>, token: String, dir: String) -> std::io::Result<()> {
    let client = reqwest::blocking::Client::new();
    for elem in blob {
        println!("{}", elem);
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
fn update_path() {
    // Get the current PATH
    let current_path = env::var("PATH").unwrap_or_else(|_| String::new());

    // Add /bin to the PATH if it's not already included
    let new_path = if current_path.contains("/bin") {
        current_path
    } else {
        format!("{}/bin:{}", env::current_dir().unwrap().display(), current_path)
    };
    println!("{}", new_path);

    // Set the updated PATH
    env::set_var("PATH", new_path);
}
// Usage: your_docker.sh run <image> <command> <arg1> <arg2> ...
 fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let root_dir = &args[1];
    let image = &args[2];
    let command = &args[3];
    let command_args = &args[4..];

    pull(image.clone(), root_dir.to_string()).unwrap();
    
    // file system isolation
    let tmp_dir = TempDir::new()?;
    println!("Temporary directory created at: {:?}", tmp_dir.path());
    //chroot
    println!("{}", root_dir);
    chroot(root_dir)?;
    //chroot(tmp_dir.path())?;
    std::env::set_current_dir("/")?;
    std::process::Command::new("/bin/ls")
    .arg("/bin")
    .output()
    .expect("Failed to list binaries in chroot");
    update_path();

    


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
