use std::io::Result;

fn main() -> Result<()> {
    #[cfg(feature = "fuz")]
    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .compile(&["schema/fuz/web_manga_viewer.proto"], &["src/schema/fuz/"])?;
    Ok(())
}
