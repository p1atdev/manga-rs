use std::io::Result;

fn main() -> Result<()> {
    #[cfg(feature = "fuz")]
    prost_build::compile_protos(&["schema/fuz/web_manga_viewer.proto"], &["src/schema/fuz/"])?;
    Ok(())
}
