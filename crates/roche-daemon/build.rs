// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use local proto copy for crates.io packaging; fall back to workspace proto for local dev.
    let (proto_file, include_dir) = if std::path::Path::new("proto/roche/v1/sandbox.proto").exists()
    {
        ("proto/roche/v1/sandbox.proto", "proto")
    } else {
        ("../../proto/roche/v1/sandbox.proto", "../../proto")
    };

    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(&[proto_file], &[include_dir])?;
    Ok(())
}
