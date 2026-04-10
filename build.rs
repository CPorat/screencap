use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=swift/Sources");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_ARCH");
    println!("cargo:rerun-if-env-changed=TARGET");

    if env::var_os("CARGO_FEATURE_MOCK_CAPTURE").is_some() {
        return;
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    if let Err(error) = compile_swift_bridge() {
        panic!("failed to build Swift bridge: {error}");
    }
}

fn compile_swift_bridge() -> Result<(), String> {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|error| error.to_string())?);
    let swift_dir = manifest_dir.join("swift/Sources");
    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|error| error.to_string())?);

    let swift_sources = collect_swift_sources(&swift_dir)?;
    if swift_sources.is_empty() {
        return Err(format!(
            "no Swift sources found under {}",
            swift_dir.display()
        ));
    }

    let sdk_path = run_xcrun(["--sdk", "macosx", "--show-sdk-path"])?;
    let swiftc_path = run_xcrun(["--find", "swiftc"])?;
    let swift_target = swift_target_triple()?;
    let library_path = out_dir.join("libscreencap_swift.a");

    let mut swiftc = Command::new(&swiftc_path);
    swiftc
        .arg("-parse-as-library")
        .arg("-module-name")
        .arg("ScreencapSwiftBridge")
        .arg("-target")
        .arg(&swift_target)
        .arg("-emit-library")
        .arg("-static")
        .arg("-o")
        .arg(&library_path)
        .arg("-sdk")
        .arg(sdk_path.trim())
        .args(&swift_sources);

    run_command(swiftc, "swiftc")?;

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=screencap_swift");

    for framework in ["AppKit", "CoreGraphics", "Foundation", "ScreenCaptureKit"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }

    if let Some(swift_runtime_dir) = swift_runtime_dir(Path::new(swiftc_path.trim())) {
        println!(
            "cargo:rustc-link-search=native={}",
            swift_runtime_dir.display()
        );
    }

    Ok(())
}

fn collect_swift_sources(swift_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut sources = Vec::new();
    let entries = fs::read_dir(swift_dir)
        .map_err(|error| format!("failed to read {}: {error}", swift_dir.display()))?;

    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to read Swift dir entry: {error}"))?;
        let path = entry.path();
        if path.extension() == Some(OsStr::new("swift")) {
            println!("cargo:rerun-if-changed={}", path.display());
            sources.push(path);
        }
    }

    sources.sort();
    Ok(sources)
}

fn run_xcrun<const N: usize>(args: [&str; N]) -> Result<String, String> {
    let mut command = Command::new("xcrun");
    command.args(args);
    let output = command
        .output()
        .map_err(|error| format!("failed to invoke xcrun: {error}"))?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .map(|value| value.trim().to_owned())
            .map_err(|error| format!("xcrun output was not utf-8: {error}"))
    } else {
        Err(format!(
            "xcrun {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn run_command(mut command: Command, label: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("failed to invoke {label}: {error}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{label} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn swift_runtime_dir(swiftc_path: &Path) -> Option<PathBuf> {
    let usr_dir = swiftc_path.parent()?.parent()?;
    Some(usr_dir.join("lib/swift/macosx"))
}

fn swift_target_triple() -> Result<String, String> {
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        Ok(other) => return Err(format!("unsupported macOS target architecture: {other}")),
        Err(error) => return Err(format!("missing CARGO_CFG_TARGET_ARCH: {error}")),
    };

    Ok(format!("{arch}-apple-macos14.0"))
}
