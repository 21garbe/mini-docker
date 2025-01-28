use anyhow::{Context, Result, anyhow};
// use std::fs::{copy, create_dir_all};
// use std::os::unix::fs::chroot;
// use tempfile::TempDir;
// use std::path::Path;
// use std::env;
use serde_json::{Value, json};
use futures::executor::block_on;
use serde::Deserialize;


// Usage: your_docker.sh run <image> <command> <arg1> <arg2> ...
// fn main() -> Result<()> {
//     let args: Vec<_> = std::env::args().collect();
//     // let image = &args[2]; 
// 
//     // here must create a temp dir and put the command in it
//     // then call chroot to this temp dir and then execute the 
//     // command as done bellow  
// 
//     let command = &args[3];
//     let command_args = &args[4..];
// 
// 
// 	let tmp_dir = TempDir::new()?;
// 	create_dir_all(tmp_dir.path().join("dev/null"))
// 		.context("failed in creating null device")?;
// 	let dst = tmp_dir
// 		.path()
// 		.join(command.split("/").last().unwrap());
// 
// 	println!("the new location of the command is \n:{}", dst.to_string_lossy().to_string());
// 
// 	let resolved = resolve_name(command).context("failed to resolving name of command")?;
// 	copy(resolved, dst).context("failed to copy")?;	
// 
// 	chroot(tmp_dir.path()).context("failed to chroot")?;
// 	env::set_current_dir("/").context("failed to set cur dir to /")?;	
// 
// //	unsafe {libc::unshare(libc::CLONE_NEWPID)}
// 
//     let output = std::process::Command::new(command.split("/").last().unwrap())
//         .args(command_args)
//         .output()
//         .with_context(|| {
//             format!(
//                 "Tried to run '{}' with arguments {:?}",
//                 command, command_args
//             )
//         })?;
//     
//     if output.status.success() {
//         let std_out = std::str::from_utf8(&output.stdout)?;
//         print!("{}", std_out);
// 	let std_err = std::str::from_utf8(&output.stderr)?;
// 	eprint!("{}", std_err);
//     } else {
//         let exit_code = output.status.code().unwrap_or(-1);
// 	if exit_code==-1 {
// 		std::process::exit(0);
// 	} else {
// 		println!("exit with code {}",  exit_code);
// 		std::process::exit(exit_code);
// 	}
//     }
// 
//     Ok(())
// }
// 
// fn resolve_name(command: &str) -> Result<String> {
// 	println!("command name is {}", command.to_string());
//     if Path::new(command).is_absolute() || command.contains('/') {
// 		if Path::new(command).exists() {
// 			return Ok(command.to_string());
// 		} else {
// 			return Err(anyhow!("doesn't exist"));
// 		} 
// 	} else if let Ok(paths) = env::var("PATH") {
// 		for path in paths.split(':') {
// 			let full_path = Path::new(path).join(command);
// 			if full_path.exists() && full_path.is_file() {
// 				return Ok(full_path.to_string_lossy().to_string());
// 			}
// 		}
// 	} 
// 	println!("error in resolving the name of the command"); 
// 	return Err(anyhow!("error resolving path"));
// }

#[derive(Deserialize)]
struct token_resp {
	token: String,
}

// #[derive(Deserialize)]
// struct manifest_resp {
// 	layers: String, 
// }

async fn get_token(client: &reqwest::Client, image: &str) -> Result<String> {
//	let client = reqwest::Client::new();

	let token_request = format!(
		 "https://auth.docker.io/token?service=registry.docker.io&scope=repository:library/{}:pull",
		 image);

	let response = client
		.get(token_request)
		.send()
		.await
		.context("failed fetching the token")?;

	let res_json: token_resp = response.json().await?;

	Ok(res_json.token)	
}

async fn get_manifest(client: &reqwest::Client, token: &str, image: &str) -> Result<()> {
	let manifest_request = format!(
		"https://registry.hub.docker.com/v2/library/{}/manifests/latest",
		image,
	);

	let manifest = client.get(manifest_request)
		.header("Accept", "application/vnd.docker.distribution.manifest.v2+json")
		.header("Authorization", format!("Bearer {}", token))
		.send()
		.await
		.context("failed fetching the manifest")?;

	let manifest_json: Value = manifest.json().await?;
	let prettylayer = serde_json::to_string_pretty(&manifest_json)?;
//	println!("the manifest layer: {}", prettylayer);

	if let Some(manifests) = manifest_json.get("manifests") {
		let digest = get_digest(manifests).await?;
		let spec_manifest_url = format!(
			"https://registry.hub.docker.com/v2/library/{}/manifests/{}",
			image, digest); 
		let specific_manifest = client.get(&spec_manifest_url)
			.header("Accept", "application/vnd.docker.distribution.manifest.v2+json")
			.header("Authorization", format!("Bearer {}", token))
			.send()
			.await
			.context("failed fetching the specific manifest")?;

		let specific_json: Value = specific_manifest.json().await?;
		let prettylayer2 = serde_json::to_string_pretty(&specific_json)?; 	
		println!("here is the specific manifest {}", prettylayer2);

	}	

	Ok(())	
}

async fn get_digest(manifests: &Value) -> Result<&str> {
	// Sélectionne un manifest spécifique (exemple : amd64)
	if let Some(selected_manifest) = manifests.as_array().and_then(|m| {
	    m.iter().find(|manifest| {
	        manifest
	            .get("platform")
	            .and_then(|platform| platform.get("architecture"))
	            == Some(&Value::String("amd64".to_string()))
	    })
	}) {
	    let digest = selected_manifest
	        .get("digest")
	        .and_then(Value::as_str)
	        .context("No digest found for selected manifest")?;
	
		Ok(digest)
	} else {
		Err(anyhow!("couldn't get the digest"))
	}
}


#[tokio::main]
async fn main() -> Result<()> {
	let client = reqwest::Client::new();
	let token: String = get_token(&client, "alpine").await?;
	get_manifest(&client, &token, "alpine").await?;
//	println!("The response token is {}", token);
 	Ok(())
}
