# manga-rs

An experimental Rust downloader for manga published through official web viewers.

The CLI inspects the episode page and detects its viewer implementation from the
HTML structure. It does not require the website's domain to be registered in
advance, so compatible viewers hosted on other domains can also be handled.

## Installation

```bash
cargo install --git https://github.com/p1atdev/manga-rs cli
```

## Usage

```bash
❯ manga --help
Download manga from supported official web viewers

Usage: manga <COMMAND>

Commands:
  episode  Download a single episode
  series   Download the available episodes in a GigaViewer series
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

When working from this repository, the project-local Cargo alias runs the CLI
without installing it:

```bash
cargo manga --help
cargo manga episode <URL>
```

Downloads are saved as WebP images in a new directory under the current
directory by default. The common download options are:

- `-o, --output-dir <DIR>`: parent directory for downloaded output (default: `.`)
- `-s, --save-as <FORMAT>`: `raw`, `zip`, or `cbz` (default: `raw`)
- `-f, --format <FORMAT>`: `png`, `jpeg`, or `webp` (default: `webp`)
- `--no-progress`: disable progress bars
- `-j, --jobs <COUNT>`: maximum concurrent image-processing jobs (default: logical CPU count)
- `--connections <COUNT>`: maximum concurrent network requests (default: `8`)

### Examples

- download an episode as a [cbz](https://en.wikipedia.org/wiki/Comic_book_archive) file

> [!TIP]
> `.cbz` is a just a `.zip` file with a different extension. You can rename it to `.zip` and extract it.

```bash
manga episode https://tonarinoyj.jp/episode/2550912964641693231 \
    --output-dir ./output \
    --save-as cbz
```

- download available multiple episodes as image files

```bash
manga series https://shonenjumpplus.com/episode/17106371853091617526 \
    --output-dir ./output
```

## Supported Websites

- [x] [ChojuGiga Viewer](https://hatena.co.jp/solutions/gigaviewer) family: episode and series downloads are supported
  - [Shonen Jump Plus](https://shonenjumpplus.com)
  - [Tonari no Young Jump](https://tonarinoyj.jp)
  - [Shonen Jump Magazine Pocket](https://pocket.shonenmagazine.com)
  - [Comic Days](https://comic-days.com)
  - [Kurage Bunch](https://kuragebunch.com)
  - [Comic Heros](https://viewer.heros-web.com)
  - [Comic Border](https://comicborder.com)
  - [Comic Gardo](https://comic-gardo.com)
  - [Comic Zenon](https://comic-zenon.com)
  - [Magcomi](https://magcomi.com)
  - [Comic Action](https://comic-action.com)
  - [Comic Trail](https://comic-trail.com)
  - [Comic Growl](https://comic-growl.com)
  - [Feelweb](https://feelweb.jp)
  - [Sunday Webry](https://www.sunday-webry.com)
  - [Comic Ogyaaa](https://comic-ogyaaa.com)
  - [Comic Earthstar](https://comic-earthstar.com)
  - [Ourfeel](https://ourfeel.jp)
  - [Ichijin Plus](https://ichijin-plus.com) (now detected as a GigaViewer-compatible site)
- [x] [Kadokomi (former ComicWalker)](https://comic-walker.com): episode download is supported
- [x] [Comici Viewer](https://comici.co.jp/business/comici-plus) family: episode and series downloads are supported
  - [Bibibi Comic](https://bibibi-comic.com)
  - [Young Champion](https://youngchampion.jp)
- [ ] [Comic FUZ](https://comic-fuz.com): deferred because its protobuf API is currently unstable; library code is available behind the `fuz` feature
- [ ] [Manga Library Z](https://www.mangaz.com): deferred; library code is available behind the `mangaz` feature
- [ ] [Piccoma](https://piccoma.com)

## Architecture

- `crates/cli`: detects the viewer and dispatches to the matching download pipeline.
- `crates/manga/src/viewer`: viewer-specific page parsing, API access, and image solving.
- `crates/manga/src/pipeline.rs`: shared concurrent fetch, solve, and write orchestration.
- `crates/manga/src/io`: raw-directory and ZIP/CBZ writers.

The `manga` crate enables `comici`, `giga`, and `kadokomi` by default. Live-site smoke tests
are ignored during normal test runs; fixture-based parsing and image-processing
tests run offline.
