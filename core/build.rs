// Pass the browser app version into sent-core so update checks match the binary.
fn main() {
    let ver = std::env::var("SENTINEL_APP_VERSION")
        .or_else(|_| {
            // Prefer workspace root Cargo.toml version when building the app
            std::fs::read_to_string("../Cargo.toml")
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.trim().starts_with("version ="))
                        .and_then(|l| {
                            l.split('"').nth(1).map(|v| v.to_string())
                        })
                })
                .ok_or(())
        })
        .unwrap_or_else(|_| "0.0.0".into());
    println!("cargo:rustc-env=SENTINEL_APP_VERSION={}", ver);
    println!("cargo:rerun-if-changed=../Cargo.toml");
}
