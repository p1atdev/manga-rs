#[cfg(feature = "fuz")]
use manga::viewer::fuz;

fn main() {
    println!(
        "Req: {:?}",
        fuz::web_manga_viewer::WebMangaViewerRequest::default()
    );

    println!(
        "Res: {:?}",
        fuz::web_manga_viewer::WebMangaViewerResponse::default()
    );
}
