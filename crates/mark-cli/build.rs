use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=MARK_BUILD_CHANNEL");
    println!("cargo:rerun-if-env-changed=MARK_BUILD_COMMIT");

    let version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION should be set");
    let Some(channel) = env::var("MARK_BUILD_CHANNEL")
        .ok()
        .map(|channel| sanitize_version_component(&channel))
        .filter(|channel| !channel.is_empty())
    else {
        println!("cargo:rustc-env=MARK_CLI_VERSION={version}");
        return;
    };

    let commit = env::var("MARK_BUILD_COMMIT")
        .ok()
        .map(|commit| sanitize_version_component(&short_commit(&commit)))
        .filter(|commit| !commit.is_empty());

    let version = match commit {
        Some(commit) => format!("{version}-{channel}+{commit}"),
        None => format!("{version}-{channel}"),
    };
    println!("cargo:rustc-env=MARK_CLI_VERSION={version}");
}

fn sanitize_version_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '0'..='9' | 'A'..='Z' | 'a'..='z' | '-' | '.' => ch,
            _ => '-',
        })
        .collect::<String>()
        .trim_matches(['-', '.'])
        .to_owned()
}

fn short_commit(value: &str) -> String {
    value.chars().take(12).collect()
}
