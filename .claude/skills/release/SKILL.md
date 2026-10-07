---
name: release
description: typelisp の新しい版(例 0.2.0)をリリースする。版上げ、ABI 再生成、翻訳の鮮度、main へのマージ、予行、タグ push、公開後の確認、crates.io 公開の案内まで。「リリースしたい」「版を上げたい」「vX.Y.Z を出す」ときに使う。
---

# typelisp のリリース

正本は [docs/dev/development.md](../../../docs/dev/development.md) の「リリースの手順」と
`.github/workflows/release.yml`。この文書は、その前後で人が踏む手順と、実際に踏んで分かった
注意点を足したもの。食い違ったら正本を直し、ここも直す。

引数は新しい版 `X.Y.Z`。以降 `<ver>` と書く。

## 何が自動で、何が人の仕事か

| 自動(タグ `v*` の push で release.yml が走る) | 人の仕事 |
|---|---|
| 全環境のテスト(各 4 分割、約 1.5 時間) | 版上げのコミット、ABI 再生成 |
| macOS 2 CPU の配布物のビルドと GitHub リリース公開 | 翻訳元コミットの更新 |
| `install.sh` で入れて版・JIT・AOT を確認 | 予行の起動、タグ打ち |
| ドキュメントサイトの再生成(`docs.yml`) | リリースノートの書き直し(任意) |
| | **crates.io への公開(ユーザーが流す。Claude は流さない)** |

テストの全実行を手元でやらない。数時間かかり、CI が同じものを走らせる。

## 手順

1. **事前確認**: `git status` がクリーン、`main` が最新、直近の CI が緑。
   `gh run list --branch main --limit 3` で見る。作業は `release/v<ver>` ブランチで行う。

2. **版を上げる**(下の「版の出現箇所」を全部)。`grep -rn '<旧版>' -I .` で
   `target/` `graphify-out/` `Cargo.lock` を除いて洗い出し、取りこぼしを探す。

3. **ABI を再生成**: `scripts/regen-abi-version.sh`(約 30 秒)。
   - 本体の MAJOR.MINOR が変わったときは `docs/dev/api_version/history/api_<ver>.md` が新しく
     でき、`latest_api_signature.md` と `crates/typelisp-abi/src/lib.rs` の `with_abi_version!` が
     変わる。PATCH だけの版上げでは何も書かれないことがある(それで正しい)。
   - `history/` の既存ファイルは削除も編集もしない。
   - ついでに `Cargo.lock` も更新される。

4. **テスト(版に関わる分だけ)**:
   `scripts/with-llvm-env.sh cargo test --test abi_version_test --test typl_help_version_test`

5. **翻訳の鮮度**: README_JP.md を変えたので README.md(英)が STALE になる。
   版上げをコミットし、そのコミットのハッシュを README.md の 1 行目
   `<!-- translated-from: README_JP.md @ <ハッシュ> -->` に書いて別コミットにする
   (`scripts/check-translations.sh` が「all N translations are up to date」になること)。
   他言語は `docs/ja` 由来なので、README_JP.md の版の記述では STALE にならない。

6. **crates.io の事前確認**: `scripts/with-llvm-env.sh cargo publish --dry-run -p typelisp-mem --allow-dirty`。
   通せるのは先頭のクレートだけ(残りは前のクレートが crates.io の索引に載ってからでないと
   依存が見つからない)。

7. **main へ**: `git merge --no-ff release/v<ver>` して push。このリポジトリは作業ブランチを
   `Merge branch '...'` で取り込む。マージ後ブランチは `git branch -d`。

8. **予行**: `gh workflow run release.yml --ref main`。公開は何もせず、テストと配布物の
   ビルドまでで止まる。約 1.5 時間。`gh run watch <id> --exit-status` をバックグラウンドで待つ。
   結果の見方: 全体が `success` なら良い。`Publish the release` / `Documentation` / `Install`
   が success 以外に見えるのは、手動起動ではスキップされる設計なので失敗ではない。

9. **タグ**: 予行と main の CI が緑なら `git tag -a v<ver> -m "v<ver>"` → `git push origin v<ver>`。
   既存タグ(`v0.1.0`, `v0.1.1`)は注釈付き。タグの push は外に出る操作なので、
   ユーザーがこのリリースを依頼した文脈でなければ、打つ前に確認を取る。
   release.yml はタグと `[workspace.package]` の版が違うと止まる。

10. **公開後の確認**(タグの run が約 1.5 時間):
    - `gh run view <id>` で Publish / Documentation / Install(arm64, x86_64)が success。
    - `gh release view v<ver>` に両 CPU の tar.gz と SHA-256 がある。
    - ドキュメントサイトに新しい MAJOR.MINOR が加わった(MINOR が上がったときだけ増える。
      PATCH では同じ版のページが最新パッチで置き換わる)。
      `curl -s https://junsuzukijapan.github.io/typelisp/en/versions.json` に新しい版があること。
    - **Documentation ジョブが失敗していたら**: v0.2.0 では、`github-pages` 環境のデプロイ許可が
      ブランチ `main` だけでタグを拒み、このジョブが落ちた(ステップが 0 件のまま落ちるのでログは
      出ない。アノテーションに `Tag "v..." is not allowed to deploy to github-pages due to
      environment protection rules` と出る:
      `gh api repos/JunSuzukiJapan/typelisp/check-runs/<job id>/annotations`)。
      恒久対策として環境にタグの許可 `v*` を足した(2026-10-07)。次の版からは通るはずだが、
      `gh api repos/JunSuzukiJapan/typelisp/environments/github-pages/deployment-branch-policies`
      に `v*`(tag)があることは確かめる。それでも落ちたら `gh workflow run docs.yml --ref main`
      で手動起動する(タグにあるものがそのまま公開される。約 1.5 分)。
    - リリースノートは `--generate-notes` のコミット一覧。書き直すなら `gh release edit v<ver> --notes-file ...`。

11. **crates.io の案内**: 公開は依存される側から順に、ユーザーが流す。
    typelisp-mem → typelisp-abi → typelisp-print → typelisp-read → typelisp-rt → typelisp-front
    → typelisp。`scripts/publish-crates.sh` がこの順に流す(`--dry-run` は typelisp-mem だけ
    確かめて止まる。公開済みの版は飛ばすので、途中で止まっても同じコマンドで続けられる)。
    Claude は流さず、このコマンドをユーザーに渡す。
    公開後は、読み取りだけで確かめる(`publish-crates.sh` は本番の公開を行うので確認には使わない):
    7 クレートそれぞれ `curl -s -A "<名前>" https://crates.io/api/v1/crates/<crate>/<ver>` が
    `"num":"<ver>"` を返し、`"yanked":false` であること。

## 版の出現箇所(0.1.1 → 0.2.0 のときの全部)

- ルート `Cargo.toml`: `[workspace.package]` の `version` 1 つと、`[workspace.dependencies]` の
  内部クレート 6 つの `version`(各クレートの Cargo.toml は `version.workspace = true` なので触らない)
- `Cargo.lock`(regen-abi-version.sh か cargo が更新する)
- `tests/typl_help_version_test.rs` の期待値 `typl <ver>\n`
- `README.md` と `README_JP.md` のインストール例 `TYPELISP_VERSION=<ver>`
  (0.1.1 の版上げは README_JP.md しか直しておらず、README.md の分が漏れていた)
- `crates/typelisp-abi/src/lib.rs` と `docs/dev/api_version/` は regen-abi-version.sh が書く

過去の版を説明している文(`docs/dev/docs-versioning.md`、`scripts/docs/build-site.py` の
コメントの `v0.1.1` など)は旧版の事実なので直さない。

## やらないこと

- 全テストを手元で回さない(CI の仕事)。
- `cargo fmt` を打たない(リポジトリは未整形で、130 ファイルが巻き添えになる)。
- 履歴の ABI ファイルを触らない。
- `cargo publish` を Claude が流さない。
- 利用者向け文書に版数やバージョン切替のリンクを書かない(サイト側が付ける)。
