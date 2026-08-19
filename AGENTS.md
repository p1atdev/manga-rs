# manga-rs

公式のオンライン漫画サイトから漫画をスクレイピングするツール。

## 構成

Rust を使う。

- `crates/cli` ディレクトリ: CLI ツールから漫画をダウンロード
- `crates/manga` ディレクトリ: スクレイピングを行うためのライブラリ
- `playground` ディレクトリ: TypeScript を利用したテスト

TypeScript は Bun を使用。
