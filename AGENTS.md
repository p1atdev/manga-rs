# manga-rs

公式のオンライン漫画サイトから漫画をスクレイピングするツール。

## 構成

Rust を使う。

- `crates/cli` ディレクトリ: CLI ツールから漫画をダウンロード
- `crates/manga` ディレクトリ: スクレイピングを行うためのライブラリ
- `playground` ディレクトリ: TypeScript を利用したテスト

TypeScript は Bun を使用。

## サポートサイト

- [x] [ChojuGiga Viewer](https://hatena.co.jp/solutions/gigaviewer) family: Episode and Series download are supported
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
- - [Ichijin Plus](https://ichijin-plus.com)
- [x] [Comic FUZ](https://comic-fuz.com): Episode download is supported (gRPC 通信を行う)
- [ ] [Manga Library Z](https://www.mangaz.com) (サービス終了は延期された)
- [x] [Kadokomi (former ComicWalker)](https://comic-walker.com): Episode download is supported
- [ ] [Piccoma](https://piccoma.com) (難易度が高い)
