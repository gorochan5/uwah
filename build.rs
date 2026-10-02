use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(false)
        .file_descriptor_set_path(out_dir.join("uwah_descriptor.bin"))
        .compile_protos(
            &[
                "proto/uwah/v1/calls.proto",
                "proto/uwah/v1/chats.proto",
                "proto/uwah/v1/contacts.proto",
                "proto/uwah/v1/events.proto",
                "proto/uwah/v1/groups.proto",
                "proto/uwah/v1/health.proto",
                "proto/uwah/v1/media.proto",
                "proto/uwah/v1/messages.proto",
                "proto/uwah/v1/sessions.proto",
                "proto/uwah/v1/webhooks.proto",
            ],
            &["proto"],
        )?;
    Ok(())
}
