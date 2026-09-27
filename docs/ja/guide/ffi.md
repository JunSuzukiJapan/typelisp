# C FFI（defffi）

C の関数を typelisp から呼ぶ方法を説明します。宣言できる型と制約の一覧は
[構文リファレンス 3.3](../reference/syntax.md#33-defffi--c-関数の宣言ffi) にあります。

## 1. 関数を宣言して呼ぶ

`defffi` で C 関数の名前と型を宣言します。

```lisp
(defffi (c-getpid "getpid") () i32)            ; typelisp 側の名前と C のシンボル名
(defffi (c-strlen "strlen") (string) c-ulong)
(defffi (c-cos "cos") (f64) f64 :library "m")  ; libm から探す
```

呼び出しは `(unsafe ...)` で包みます。

```lisp
(unsafe (c-getpid))        ; => 12345
(unsafe (c-cos 0.0))       ; => 1.0
```

`unsafe` が必要なのは、宣言した型が C 側の本当の型と合っているかを、コンパイラが確かめられない
からです。`unsafe` と書くことで、その確認を書き手が引き受けたことになります。付け忘れると、
その旨を説明するエラーになります。

## 2. 安全なラッパを作る

`unsafe` は 1 か所に閉じ込め、外には普通の関数として見せるのが想定された使い方です。

```lisp
(defun pid () i32 (unsafe (c-getpid)))
(defun str-len ((s string)) int (as int (unsafe (c-strlen s))))

(pid)              ; 呼ぶ側に unsafe はいらない
(str-len "hello")  ; => 5
```

## 3. 型の対応

| typelisp | C |
|---|---|
| `i8` `i16` `i32` / `u8` `u16` `u32` | 同じ幅の整数 |
| `f32` / `f64` | `float` / `double` |
| `bool` | `bool`（`_Bool`） |
| `()` | `void` |
| `string` | `const char *` |
| `c-long` / `c-ulong` | `long` / `unsigned long`（`size_t` `int64_t` なども） |
| `ptr` | 任意のポインタ（`void *` `FILE *` など） |
| `(ptr T)` | `T` へのポインタ（[7 節](#7-c-の構造体)） |

### 文字列

- `string` を渡すと、NUL 終端した C 文字列にコピーしてから渡し、呼び出しが終わったら解放します。
  文字列の途中に NUL があるとエラーになります。
- `string` を返す関数の結果もコピーされます。C 側のメモリは解放しません。呼び出し側が解放すべき
  文字列を返す関数（`strdup` など）は、`ptr` で受けて自分で `free` してください。
- `string` を返すと宣言した関数が NULL を返すとエラーになります。NULL を返しうる関数
  （`getenv` など）は `ptr` で受けてください。

### `c-long` / `c-ulong` / `ptr`

これらは C との境界を渡すためだけの型で、**算術はできません**。typelisp の整数として使うときは
`as` で変換します。

```lisp
(as int (unsafe (c-strlen s)))      ; int は 64bit の値を落とさない
(try-as i32 (unsafe (c-strlen s)))  ; i32 に入らなければ none
(unsafe (c-malloc 16))              ; 整数リテラルはそのまま渡せる
```

`ptr` は C の関数に渡し返すための値です。typelisp の側で中身を読む手段はありません。

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())

(unsafe (let ((p (c-malloc 16)))
          (c-free p)
          ()))
```

これらの型は、関数の引数・戻り値・局所変数にだけ置けます。構造体のフィールド、グローバル変数、
`Vector` などの型引数には置けません。

## 4. ライブラリを指定する

`:library` を省くと、プロセスに既にリンクされているもの（libc など）からシンボルを探します。
それ以外のライブラリの関数は `:library` で指定します。

```lisp
(defffi (sqlite-version "sqlite3_libversion") () string :library "sqlite3")
```

- `"sqlite3"` のような短い名前は、`libsqlite3.dylib`、`libsqlite3.so` の順に探します。
- `/` を含む名前はパスとして扱います。
- 宣言したシンボルが見つからなければ、その名前を挙げたエラーになります。

## 5. AOT コンパイル

`defffi` を使ったプログラムも、そのまま [`compile-file`](compile.md#3-aot-コンパイルで実行ファイルを作る)
で実行ファイルにできます。`:library` で指定したライブラリはリンク時に自動で追加されるので、
`compile-file` に引数を足す必要はありません。

## 6. コールバック

C の関数に typelisp の関数を渡して、呼び返してもらえます。`defffi` の引数型に関数型を書き、
呼ぶときにはその位置に関数名か `lambda` 式を書きます。

```lisp
(defffi (c-free "free") (ptr) ())
(defffi (c-strdup "strdup") (string) ptr)
(defffi (c-strncmp "strncmp") (ptr ptr c-ulong) i32)
(defffi (text-of "strstr") (ptr string) string)          ; strstr(p, "") は p を返す
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun sort-chars ((s string)) string
  (unsafe
    (let ((buf (c-strdup s)))
      (c-qsort buf (as c-ulong (length s)) 1
               (lambda ((a ptr) (b ptr)) i32 (c-strncmp a b 1)))
      (let ((r (text-of buf ""))) (c-free buf) r))))

(sort-chars "cadb")    ; => "abcd"
```

- 渡せるのは、**自由変数の無い**関数だけです。トップレベル関数、`lambda`、`labels` の局所関数の
  どれでも使えますが、外側の局所変数を参照していると型検査でエラーになります。C は宣言した
  引数しか渡さないので、捕捉した変数を届ける方法が無いためです。状態を持たせたいときは
  グローバル変数を使います。
- 関数を入れた変数は渡せません。その場に関数名か `lambda` 式を書いてください。
- コールバックの中で起きた `panic` や `throw` は、C の関数が戻った後で呼び出し元に伝わります。
- 呼び返せるのは、typelisp が呼んだ C の関数が走っている間だけです。`atexit` やシグナル
  ハンドラから呼ばれるような使い方はできません。

## 7. C の構造体

C の関数に構造体の配列などを渡したいときは、`def-c-struct` で C と同じ配置の構造体を宣言し、
`unsafe` の中で確保します。

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))    ; トップレベルの unsafe の中で宣言する

(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(defun put ((xs (ptr item)) (i int) (k i32)) i32
  (unsafe (let ((p (c-ref xs i))) (setf p::key k))))

(defun key-at ((xs (ptr item)) (i int)) int
  (unsafe (let ((p (c-ref xs i))) (as int p::key))))

(defun sorted-keys () int
  (unsafe
    (let ((xs (c-alloc item 4)))                    ; item を 4 個。中身は 0
      (put xs 0 3) (put xs 1 1) (put xs 2 4) (put xs 3 2)
      (c-qsort (as ptr xs) 4 8
               (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
      (+ (* 1000 (key-at xs 0)) (* 100 (key-at xs 1))
         (* 10 (key-at xs 2)) (key-at xs 3)))))

(sorted-keys)    ; => 1234
```

- `(c-alloc T n)` は `T` を `n` 個確保して `(ptr T)` を返します。`(c-ref p i)` は `i` 個目への
  ポインタ、`p::field` はフィールド、`(c-deref p)` は `i32` などのスカラへのポインタの中身です。
  どれも `setf` で書き込めます。
- `(as ptr p)` で型の無い `ptr` にして、`void *` を取る C の関数に渡します。
- `item` のサイズ（ここでは 8）と各フィールドの位置は、C と同じ規則で決まります。

### 確保したメモリの寿命

確保したメモリは、その関数の中で一番外側の `unsafe` を出た時点で解放されます。`panic` や
`throw` で抜けた場合も同じです。そのため `(ptr T)` の値は `unsafe` の外へ持ち出せません。
`unsafe` の値にする、クロージャで捕捉する、`task` に渡す、`throw` で投げる、のどれも型検査で
エラーになります。外で使いたい値は、`unsafe` の中で数値や `defstruct` にコピーしてください。

`lambda` や `labels` の関数の中で確保するときは、その中に `unsafe` を書きます。

### C が確保したメモリ

`(ptr T)` として C から受け取ったポインタ（`defffi` の戻り値、コールバックの引数など）は、
`c-alloc` で確保したメモリの中を指していなければエラーになります。C が `malloc` したメモリや
NULL を受け取る関数は、型の無い `ptr` で宣言してください。

## 8. できないこと

- **可変長引数の関数**（`printf` など）は宣言できません。可変長部分は固定引数と別の規則で
  渡されるためです。使う引数の個数ごとに別の名前で宣言してください。
- **構造体の値渡し・値返し**はできません。ポインタで受け渡す関数を使ってください。
- **ジェネリックな宣言**はできません。
- **組み込み関数と同じ名前**は付けられません。
- **関数値として渡せません。** `(map xs c-abs)` のように渡すことはできないので、`lambda` で包みます。

  ```lisp
  (unsafe (lambda ((n i32)) i32 (c-abs n)))
  ```

