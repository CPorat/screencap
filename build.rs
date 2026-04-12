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
    println!("cargo:rerun-if-env-changed=SCREENCAP_WEB_DEV");
    register_web_rerun_hints();
    if let Err(error) = build_web_ui() {
        panic!("failed to build web frontend: {error}");
    }

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

fn register_web_rerun_hints() {
    for path in [
        "web/package.json",
        "web/package-lock.json",
        "web/tsconfig.json",
        "web/svelte.config.js",
        "web/vite.config.ts",
        "web/index.html",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }

    print_rerun_if_changed_tree(Path::new("web/src"));
    print_rerun_if_changed_tree(Path::new("web/public"));
}

fn print_rerun_if_changed_tree(path: &Path) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            print_rerun_if_changed_tree(&entry_path);
        } else {
            println!("cargo:rerun-if-changed={}", entry_path.display());
        }
    }
}

fn build_web_ui() -> Result<(), String> {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|error| error.to_string())?);
    let web_dir = manifest_dir.join("web");
    let dist_dir = web_dir.join("dist");

    if !web_dir.exists() {
        return Err(format!(
            "missing web frontend directory at {}",
            web_dir.display()
        ));
    }

    if web_dev_mode_enabled() {
        ensure_dist_placeholder(&dist_dir)?;
        println!(
            "cargo:warning=SCREENCAP_WEB_DEV is set; skipping `npm run build` and embedding existing web/dist assets."
        );
        return Ok(());
    }

    if !command_available("npm") {
        ensure_dist_placeholder(&dist_dir)?;
        println!(
            "cargo:warning=npm is not installed; embedding placeholder UI. Install npm and run `npm run build --prefix web` to embed the full frontend."
        );
        return Ok(());
    }

    if !web_dir.join("node_modules").exists() {
        let mut npm_ci = Command::new("npm");
        npm_ci
            .current_dir(&web_dir)
            .arg("ci")
            .arg("--no-audit")
            .arg("--no-fund");
        run_command(npm_ci, "npm ci")?;
    }

    let mut npm_build = Command::new("npm");
    npm_build
        .current_dir(&web_dir)
        .arg("run")
        .arg("build:embed");
    run_command(npm_build, "npm run build:embed")?;

    if !dist_dir.join("index.html").exists() {
        return Err(format!(
            "frontend build succeeded without emitting {}",
            dist_dir.join("index.html").display()
        ));
    }

    Ok(())
}

fn web_dev_mode_enabled() -> bool {
    env::var("SCREENCAP_WEB_DEV")
        .map(|value| {
            let trimmed = value.trim();
            trimmed == "1" || trimmed.eq_ignore_ascii_case("true")
        })
        .unwrap_or(false)
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn ensure_dist_placeholder(dist_dir: &Path) -> Result<(), String> {
    let index_path = dist_dir.join("index.html");
    if index_path.exists() {
        return Ok(());
    }

    fs::create_dir_all(dist_dir)
        .map_err(|error| format!("failed to create {}: {error}", dist_dir.display()))?;
    fs::write(
        &index_path,
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>screencap</title></head><body><h1>Screencap UI is not built</h1><p>Install npm and run <code>npm run build --prefix web</code>, then rebuild.</p></body></html>",
    )
    .map_err(|error| format!("failed to write {}: {error}", index_path.display()))?;

    Ok(())
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

    for framework in [
        "AppKit",
        "ApplicationServices",
        "CoreGraphics",
        "Foundation",
        "ScreenCaptureKit",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }

    let runtime_dirs = swift_runtime_dirs(Path::new(swiftc_path.trim()));
    if runtime_dirs.is_empty() {
        println!(
            "cargo:warning=unable to locate Swift runtime dylibs; tests may fail to launch if the runtime is not discoverable."
        );
    }

    for swift_runtime_dir in runtime_dirs {
        println!(
            "cargo:rustc-link-search=native={}",
            swift_runtime_dir.display()
        );
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,{}",
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

fn swift_runtime_dirs(swiftc_path: &Path) -> Vec<PathBuf> {
    if let Some(toolchain_runtime_dir) = swift_runtime_dir_from_toolchain(swiftc_path) {
        if toolchain_runtime_dir
            .join("libswift_Concurrency.dylib")
            .exists()
        {
            return vec![toolchain_runtime_dir];
        }
    }

    let mut runtime_dirs = command_line_tools_runtime_dirs();
    runtime_dirs.retain(|path| path.join("libswift_Concurrency.dylib").exists());
    runtime_dirs.sort();
    runtime_dirs.dedup();
    runtime_dirs
}

fn swift_runtime_dir_from_toolchain(swiftc_path: &Path) -> Option<PathBuf> {
    let usr_dir = swiftc_path.parent()?.parent()?;
    Some(usr_dir.join("lib/swift/macosx"))
}

fn command_line_tools_runtime_dirs() -> Vec<PathBuf> {
    let root = Path::new("/Library/Developer/CommandLineTools/usr/lib");
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut runtime_dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("swift-") {
            continue;
        }

        let runtime_dir = path.join("macosx");
        if runtime_dir.join("libswift_Concurrency.dylib").exists() {
            runtime_dirs.push(runtime_dir);
        }
    }

    runtime_dirs
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
