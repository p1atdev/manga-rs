# manga-rs

wip

## Installation

```bash
cargo install --git https://github.com/p1atdev/manga-rs
```

## Usage

```bash
❯ manga --help
Usage: manga <COMMAND>

Commands:
  episode  
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### Examples

- download an episode as a [cbz](https://en.wikipedia.org/wiki/Comic_book_archive) file

> [!TIP]
> `.cbz` is a just a `.zip` file with a different extension. You can rename it to `.zip` and extract it.

```bash
manga episode https://tonarinoyj.jp/episode/2550912964641693231 \
    --output-dir ./output \
    --save-as cbz \
    --format webp
```

## Supported Websites


- [x] [ChojuGiga Viewer](https://hatena.co.jp/solutions/gigaviewer) family
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
- [x] [Comic FUZ](https://comic-fuz.com)
- [ ] [Ichijin Plus](https://ichijin-plus.com)
- [ ] [Kadokomi (former ComicWalker)](https://comic-walker.com)
- [ ] [Piccoma](https://piccoma.com)
