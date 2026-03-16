use std::path::Path;
use std::process::Command;

fn main() {
    let frontend_dir = Path::new("frontend");

    // Only build if package.json exists (dev environments)
    if !frontend_dir.join("package.json").exists() {
        return;
    }

    // Rerun if frontend source changes
    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/index.html");
    println!("cargo:rerun-if-changed=frontend/vite.config.ts");

    // Check if node_modules exists
    if !frontend_dir.join("node_modules").exists() {
        let status = Command::new("npm")
            .args(["install"])
            .current_dir(frontend_dir)
            .status()
            .expect("Failed to run npm install");
        if !status.success() {
            panic!("npm install failed");
        }
    }

    // Build frontend
    let status = Command::new("npm")
        .args(["run", "build"])
        .current_dir(frontend_dir)
        .status()
        .expect("Failed to run npm run build");

    if !status.success() {
        panic!("Frontend build failed");
    }
}
