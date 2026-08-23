# typelisp 開発 TODO

最終更新: 2026-08-22 / ブランチ: `feature/cl-parity`

このドキュメントは**現在残っている作業のみ**を記録する。

## 残っている作業

**CL 残差を埋める実装計画**（[cl-parity-plan.md](cl-parity-plan.md)、2026-08-20 策定）。
Common Lisp にあって typelisp に無いものを Phase 0〜9 に落とした計画。
着手順の目安は同計画の「実施順序と依存」節。

進捗:

| Phase | 状態 |
|---|---|
| 0（実装前に確かめる 6 件） | **完了** 2026-08-20。結果と、それが計画本体に強いた訂正は同計画の該当節 |
| 1（数値層 1a〜1d） | **全完了**（1a/1b/1d は 2026-08-21）。1c の保留 2 項目（乱数のシード指定・`ldb` 系の幅拡張）はそのまま。1d の complex は prelude の `defstruct`（成分は `f64` 固定、`(sqrt -1.0)` は実数 NaN のまま——[implementation-log.md](implementation-log.md) 該当節）。Phase 2 が残した「char/int の native lowering 5 つ」は未着手——1a と同じ島の分岐を触る作業 |
| 2（文字・文字列 2a/2b） | **完了** 2026-08-20 |
| 3（リスト・シーケンス 3a〜3e） | **全完了**（3e は 2026-08-21）。3e は Phase 5b に依存しなかった——対象は全部ジェネリック `defun` で、`defun` の `&key` は既に通っていた。まだキーワードを取れないのは `defmethod` の側（破壊的操作と `search`/`mismatch`）——Phase 5b で機構は入ったので、あとは対象の書き換えだけ |
| 4（制御構造・マクロ） | **4a 部分完了** 2026-08-20（`prog1`/`prog2`/`do*`/`ecase`/`ccase`/`setq`/`psetq`/`psetf`/`pushnew`）。残るは `block`/`return-from`（島の引数引き回しに全面的に触る）・`prog`/`prog*`・`destructuring-bind`・`sleep`、および `loop` の `:named`（`block` 依存）。**4b 完了** 2026-08-21（`:named` を除く拡張 `loop`）。**4c 部分完了** 2026-08-21（`macroexpand`/`macroexpand-1`、`complement`、`gensym` のプレフィクスと `*gensym-counter*`）——残るは `macrolet`/`symbol-macrolet` のみで、やり方は同計画に書いた（他の項目は「入れない」理由つきで確定） |
| 5（定義形の拡張 5a〜5c） | **5b 完了** 2026-08-21（`defmethod` が `&optional`/`&key`/`&rest` を取る。`lambda`/`labels` とトレイトのメソッドは「入れない」理由つきで確定）。**5c 完了** 2026-08-21（`deftype`。型の綴りであって型ではない——展開は型パーサの中で起き、エラーメッセージも展開後を見せる）。**5a 完了** 2026-08-21（オプションリスト・スロットのデフォルト・`:constructor`・`:copier`・`:include`。`:conc-name` と `:predicate` は「入れない」理由つきで確定）。**Phase 5 全完了** |
| 6（コレクション 6a〜6c） | **6a 部分完了** 2026-08-22（`Hash` トレイト＋`sxhash`、鍵のハッシュ可能性が静的に、`maphash`/`size`）。残るはユーザ定義型を鍵にする分（mem 層のバケットが要る）。**6b 完了** 2026-08-22（`Array<T>`。`Vector` 2 本の上の prelude `defstruct` で、Rust 側の追加はゼロ。`(aref a i j)` だけ checker の糖衣）。**6c 完了** 2026-08-22（`BitVector`。1 語 **32bit** ——64bit は上の整数切り詰めに当たる） |
| 7（エラーと動的束縛 7a/7b） | **全完了** 2026-08-22。7a は `SimpleError`/`WrappedError`/`wrap-error`/`describe-error`/`assert`/`warn`（コンディションシステムは予定どおり非採用）。7b は `dlet`（保存→代入→`unwind-protect` で復元）と印字制御変数一式＋`with-standard-io-syntax`、副産物で radix リーダマクロ `#b`/`#o`/`#x`/`#NNr`。入れなかった変数は同計画の表に 1 つずつ理由つき |
| 8 | 未着手 |
| 9（シンボル・パッケージ・環境） | **9c 完了** 2026-08-20（コマンドライン引数・環境変数・ファイルシステム問い合わせ・日時の分解合成・`y-or-n-p`）。保留は `libc` が要る 4 群と REPL ツール層。9a/9b/9d 未着手 |
| 付録 C（小さな不整合 4 件） | **完了** 2026-08-20 |
| 付録 D（範囲外の既存問題 2 件） | **完了** 2026-08-21。D-2 は `Heap::cons` の成長条件（回収後の空きが 1/4 未満なら伸ばす）、D-1 は AOT 実行ファイルが prelude を持ち歩くように |

2026-08-22 に Phase 6b/6c の締めとして**直列の全実行**（`scripts/test-serial.sh`、97 本）を
初めて回したところ、6b/6c と無関係な赤が 3 件出た。3 件とも前フェーズの取りこぼしで、
`cargo check` は通っていた。内訳と、うち 1 件が本物の言語バグだった話は
[implementation-log.md](implementation-log.md) の該当節。**フェーズの締めは「関係しそうな
テストを選んで回す」ではなく直列の全実行にする。**

Phase 3e の作業中に見つけた**コンパイラの穴 1 件（未修正）**: `lambda` が `match` の
アーム束縛を捕獲すると compile できない。

```lisp
(defun mk ((o Option<i32>)) (fn (i32) bool)
  (match o
    ((some g) (lambda ((a i32)) bool (< a g)))   ; ← `g` を捕獲
    ((none) (lambda ((a i32)) bool false))))
(compile ...)  ; => compile: `g` is referenced but no binder in scope states
               ;    its representation (internal error)
```

インタプリタでは動く。原因は `core_bridge::translate_match` がアームの本体を `cx` を
広げずに変換することで、パターン束縛の `Repr` がスコープに入らない
（`captured_with_reprs` が引ける表に無い）。`let` 束縛の捕獲は通る。
直すには `Repr` をスコープに入れるだけでなく**島側でその束縛をセル化**する必要があるので、
一行では済まない。Phase 3e の prelude はこの形を避けて書いてある。

Phase 6a の作業中に**コンパイル済みコードの整数切り詰め**を 2 件見つけた。1 件は直した
（整数リテラル: 島へは `Sexpr` として渡るので 3bit タグを引いた 61bit しか残らず、
`4611686018427387903` が `-1` にコンパイルされていた。`float` と同じ 32bit 2 分割にして
LLVM 側で組み直す。`compile_test` に 2 本）。**残る 1 件は未修正**:

```lisp
(defvar (big i64) 4611686018427387903)
(defun rd () i64 big)
(rd)            ; => 4611686018427387903
(compile rd)
(rd)            ; => -1
```

グローバルの読み出しがコンパイル済み経路でタグ付きの語を経由するため、
±2^60 の外にある `i64` グローバルが黙って壊れる。リテラルと違って
「渡し方を変える」では済まず、コンパイル済みコードから見たグローバルの表現そのものを
触る必要がある。回避策は「幅の広い定数をグローバルに置かない」こと（prelude の
`*sxhash-mask*` は 2^30-1 にしてある）。

Phase 6c で**同じ原因の 3 件目**が出た。グローバルだけの話ではなく、
**コンテナへの格納も同じ語を経由する**:

```lisp
(defun f () i64
  (let ((v (Vector::filled 1 (the i64 0))))
    (progn (set v 0 (ash (the i64 1) (the i64 60))) (get v 0))))
(f)             ; => 1152921504606846976
(compile f)
(f)             ; => -1152921504606846976
```

境界は `typelisp-abi` の `encode`（下位 3bit がタグ、残りが整数）で、
`ash` の計算そのものも引数渡しも戻り値も 64bit のまま正しい——壊れるのは
**コンテナの要素として往復させたとき**だけ。したがって「±2^60 の外にある `i64`」は
グローバルに限らず `Vector`/`HashTable`/`defstruct` のフィールドでも黙って壊れる。

本筋の直し方は `Float`/`bignum` と同じで、payload に入らない整数を `TAG_BOXED` の箱に
逃がすこと。`encode` は今ヒープを持たない純関数なので、島の lowering と `rt_*` シム、
GC ルート、等価述語と印字、ダンプの版まで動く——1 フェーズ分の作業で、Phase 6c の
片手間には入らない。`BitVector` は 1 語 64bit ではなく **32bit** で詰めることで
この端に近寄らないようにしてある（型の側にも同じ注記がある）。

`print-object` は**ジェネリック型に対して一度も発火しない**（Phase 8a で判明）。
これは登録漏れではなく、実行時ディスパッチの前提が無いという話:

```lisp
(defstruct gen<T> (v T))
(impl print-object gen<T> (print-object ((self Self) (escape bool)) string "GEN"))
(println "~a" (gen::new 1))          ; => #<gen 1>   （"GEN" ではない）
(println "~a" (print-object (gen::new 1) true))  ; => GEN  （名前で呼べば動く）
```

プリンタは値が持つ型キーでメソッドを引くが、単型化が型引数を消しているのでキーは
`gen` であって `gen<i64>` ではない。値の側に「どの実体化なのか」が書かれていない以上、
`gen<i64>` の版と `gen<string>` の版を選び分ける材料が無い。全 T で 1 本の本体を
共有する手もあるが、`gen<T>` を印字するとは `T` を印字することなので成立しない。
**型検査は通り、名前で呼べば動き、プリンタからだけ見えない**ので、書いた人が気づかない。
`tests/printer_test.rs::print_object_does_not_reach_a_generic_type` が現状を固定しており、
直ったらそのテストが落ちる。これに依存して見送ったのが `*print-array*` と
`Array<T>` の `print-object`（cl-parity-plan.md Phase 8a）。

`~/name/` は **AOT 実行ファイルでは使えない**（Phase 8a）。ディレクティブは
メソッドを実行時の `string` で名指すので、どのメソッドに到達しうるかをコンパイル時に
言えない。対応するには全型の全メソッドを起動時に登録することになる——`print-object` は
1 トレイトの impl だけを登録すれば済む。黙って別の動作をするのではなく、その旨を
述べるエラーにしてある。

Phase 2/3 の副産物として checker のバグを 4 件見つけて直した。4 件とも
**「型変数の名前がたまたま一致したときだけ動いていた」同じ形**（詳細は同計画の Phase 0 / Phase 3 の節）:
境界越しの `Self` 戻り型、境界付きジェネリック同士の委譲、ジェネリック `defmethod` の受け手の
型パラメータ名、そして compiled 経路で `format` が `f64` パラメータを生ワードのまま渡していた件。

Phase 9c の作業中に見つけた**計画の範囲外の既存問題 2 件**（同計画の付録 D）は
2026-08-21 に両方入れた。経緯は [implementation-log.md](implementation-log.md) の
「付録 D の 2 件」節。D-2 のほうで 1 つ分かったことを残しておく:
`editor_keyword_sync_test` の 649 秒は GC スラッシュ**だけ**が原因ではなく、
修正後も 502 秒かかる。残りは 6 テストがそれぞれ prelude を JIT していることで、
この 1 本を速くしたければ次はそちらを見る。

2026-08-20 に「ダンプ」（ビットコードと型情報を 1 ファイルに対で持つ）を入れ、起動は
1.50s → 1.07s になった。経緯は [implementation-log.md](implementation-log.md) の該当節。

2026-08-19 に「呼ぶとコンパイルできなくなるもの」（[syntax.md](../syntax.md) §10）の最後の1つ
`eval` を閉じ、あの表は空になった。経緯・設計判断は
[implementation-log.md](implementation-log.md) の該当節。

作業を始めるときはここに項目を足し、終わったら（経緯・設計判断を
[implementation-log.md](implementation-log.md) へ書いたうえで）ここから消す。

残差そのものの地図は
[cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md)（TODO ではなく測定）。
そこから作った実行計画が上記の [cl-parity-plan.md](cl-parity-plan.md) で、地図の全行が
どの Phase に落ちたか（落ちていないなら理由）は同計画の付録 A にある。

## 関連ドキュメント

| 知りたいこと | 参照先 |
|---|---|
| 完了した実装の経緯・設計判断 | [implementation-log.md](implementation-log.md) |
| 言語仕様の確定事項・非採用と決めた機能 | [language-design.md](language-design.md)（非採用リストは §9） |
| Common Lisp と比べてまだ無いクラス・メソッド | [cl-missing-classes-and-methods.md](cl-missing-classes-and-methods.md) |
| それを埋める実行計画（Phase / 対象外の理由 / 完了判定） | [cl-parity-plan.md](cl-parity-plan.md) |
| ビルド・テストの実行方法 | [development.md](development.md) |
| ユーザ向けの関数・構文リファレンス | [functions.md](../functions.md) / [syntax.md](../syntax.md) |
