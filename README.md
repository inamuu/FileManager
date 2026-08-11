# FileManager

Rustと[GPUI](https://gpui.rs/)で実装した、macOS向けのモダンなFinder風ファイルマネージャーです。

## 機能

- 複数ペインを横並びにして異なるフォルダを同時に表示
- ファイル／フォルダを隣のペインへドラッグ＆ドロップしてコピー
- コピー中の進捗をアプリ内フローティングパネルに表示
- SMB・NFS・WebDAVサーバーへmacOS標準認証で接続
- 接続済みサーバーのマウント先をサイドバーへ永続保存
- `/Applications`を含むローカル／NAS上のフォルダ操作

## 必要環境

- macOS 12以降
- Rust stable
- Xcode Command Line Tools

GPUIのMetalシェーダーは実行時コンパイルを使用するため、通常のビルドでは完全版Xcodeは不要です。

## 開発

```sh
cargo run
```

バックエンドのみのテスト:

```sh
cargo test --no-default-features --lib
```

## リリースとインストール

`.app`バンドルを`dist/FileManager.app`に生成します。

```sh
scripts/release.sh
```

生成後に`/Applications/FileManager.app`へコピーする場合:

```sh
scripts/release.sh --install
```

ローカル利用向けのad-hoc署名です。第三者配布にはApple Developer IDによる署名とnotarizationを追加してください。

## 操作

- 「ペインを追加」で表示領域を右側へ追加します。ウィンドウ幅を超えた場合は横スクロールできます。
- ペイン見出し左側の`⠿`を別ペインへドラッグすると、左右の表示順を変更できます。
- 青枠のペインが、サイドバーから移動する際の操作対象です。
- ファイルはシングルクリックで選択し、ダブルクリックで開きます。
- `⌘`クリックで個別に複数選択、Shiftクリックで範囲選択できます。
- `↑` / `↓`で選択を移動し、Shiftを押しながら操作すると範囲選択できます。`Enter`で開きます。
- 右クリックメニューから開く、Finderで表示、ゴミ箱へ移動できます。
- ファイル行を別ペインへドラッグすると、そのペインのフォルダへコピーします。
- サイドバーの「サーバへ接続…」から`SMB://server/share`などを入力します。
- サーバーの認証情報はFileManagerには保存せず、macOS標準の接続処理へ委譲します。
- 転送状況は右下のパネルに表示され、進捗率と転送済み容量が100ms間隔で更新されます。
