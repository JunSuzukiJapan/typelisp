# ABI バージョン 0.3.79

`scripts/regen-abi-version.sh` が生成した文書。手で編集しない。

コンパイラが生成するコードが、ABI バージョン 0.3.79 のランタイムライブラリについて仮定していることの一覧。
ここに書かれていない前提(シムが引数をどう扱うか、生成コードがヒープを直接読む箇所など)は含まない。

内容ハッシュ (FNV-1a 64): `b05c0d2f549d73fa`

## 呼び出し規約 (typelisp_abi)

| 名前 | 値 |
|---|---|
| STATUS_RETURN | 0 |
| STATUS_CALL | 1 |
| STATUS_SUSPEND | 2 |
| STATUS_UNWIND | 3 |
| BODY_ABI_CLASSIC | 0 |
| BODY_ABI_COROUTINE | 1 |
| FRAME_VALUE_SLOT | 0 |
| FRAME_HANDLER_SLOT | 1 |
| FRAME_PC_SLOT | 2 |
| FRAME_RESERVED_SLOTS | 3 |

## 中断の種類 (typelisp_abi::call_state)

| 名前 | 値 |
|---|---|
| SUSPEND_YIELD | 0 |
| SUSPEND_SLEEP | 1 |
| SUSPEND_WAIT | 2 |
| SUSPEND_SAFEPOINT | 3 |
| SUSPEND_CHAN_NEW | 4 |
| SUSPEND_CHAN_LEN | 5 |
| SUSPEND_CHAN_CAP | 6 |
| SUSPEND_CHAN_CLOSE | 7 |
| SUSPEND_CHAN_SEND | 8 |
| SUSPEND_CHAN_RECV | 9 |
| SUSPEND_CHAN_SELECT | 10 |
| SUSPEND_IO | 11 |
| SUSPEND_IO_FOR | 12 |
| SUSPEND_TASK | 13 |
| SUSPEND_THREAD | 14 |
| SUSPEND_MAIN | 15 |

## 生成コードの本体の ABI (typelisp::compile)

| 名前 | 値 |
|---|---|
| EMITTED_BODY_ABI | 1 |

## 値の表現 (typelisp_mem::tagged)

| 名前 | 値 |
|---|---|
| LAYOUT | 2 |
| LAYOUT_THREE_BIT | 0 |
| LAYOUT_FIXNUM_ONE_BIT | 1 |
| LAYOUT_OPTION_NICHE | 2 |
| FIXNUM_SHIFT | 1 |
| FIXNUM_MIN | -4611686018427387904 |
| FIXNUM_MAX | 4611686018427387903 |
| FIXNUM_MASK | 1 |
| FIXNUM_TAG | 0 |
| LOW_MASK | 7 |
| TAG_CONS | 1 |
| TAG_SYMBOL | 3 |
| TAG_BOXED | 5 |
| TAG_SMALL | 7 |
| SMALL_MASK | 63 |
| SMALL_SHIFT | 6 |
| TAG_IMMEDIATE | 7 |
| TAG_CHAR | 15 |
| TAG_STR | 23 |
| TAG_PATH | 31 |
| BOXED_SHIFT | 3 |
| IMMEDIATE_NIL | 0 |
| IMMEDIATE_FALSE | 1 |
| IMMEDIATE_TRUE | 2 |
| NIL_WORD | 7 |

## C メモリ (typelisp_rt::c_mem)

| 名前 | 値 |
|---|---|
| KIND_I8 | 1 |
| KIND_I16 | 2 |
| KIND_I32 | 3 |
| KIND_U8 | 4 |
| KIND_U16 | 5 |
| KIND_U32 | 6 |
| KIND_RAW_I32 | 7 |
| KIND_RAW_U32 | 8 |
| KIND_F32 | 9 |
| KIND_F64 | 10 |
| KIND_BOOL | 11 |
| KIND_RAW_64 | 12 |

## 終了コード (typelisp_rt)

| 名前 | 値 |
|---|---|
| EXIT_CODE_PANIC | 1 |
| EXIT_CODE_SUCCESS | 0 |

## 既定の cons アリーナ (typelisp_rt)

| 名前 | 値 |
|---|---|
| DEFAULT_AOT_HEAP_CAPACITY | 65536 |

## ダンプ形式 (typelisp_front::dump)

| 名前 | 値 |
|---|---|
| FORMAT_VERSION | 40 |
| CONTAINER_VERSION | 1 |

## well-known シンボル (typelisp_mem::BUILTIN_SYMBOLS)

| 番号 | 名前 |
|---|---|
| 0 | `quote` |
| 1 | `unquote` |
| 2 | `unquote-splicing` |
| 3 | `the` |
| 4 | `fn` |
| 5 | `pub` |
| 6 | `where` |
| 7 | `return` |
| 8 | `&rest` |
| 9 | `&optional` |
| 10 | `&key` |
| 11 | `:dyn` |
| 12 | `=` |
| 13 | `:=` |
| 14 | `int` |
| 15 | `float` |
| 16 | `bignum` |
| 17 | `ratio` |
| 18 | `char` |
| 19 | `bool` |
| 20 | `str` |
| 21 | `unit` |
| 22 | `var` |
| 23 | `set` |
| 24 | `global` |
| 25 | `set-global` |
| 26 | `let` |
| 27 | `lambda` |
| 28 | `labels` |
| 29 | `call` |
| 30 | `assoc` |
| 31 | `apply` |
| 32 | `if` |
| 33 | `loop` |
| 34 | `break` |
| 35 | `panic` |
| 36 | `match` |
| 37 | `construct` |
| 38 | `field-get` |
| 39 | `field-set` |
| 40 | `dyn-new` |
| 41 | `dyn-upcast` |
| 42 | `dyn-call` |
| 43 | `dyn-value` |
| 44 | `catch` |
| 45 | `throw` |
| 46 | `unwind-protect` |
| 47 | `sym` |
| 48 | `fnref` |
| 49 | `methodref` |
| 50 | `compile-fn` |
| 51 | `method` |
| 52 | `pat-wild` |
| 53 | `pat-empty` |
| 54 | `pat-nonempty` |
| 55 | `pat-bind` |
| 56 | `pat-lit` |
| 57 | `pat-guard` |
| 58 | `pat-ctor` |
| 59 | `pat-typetest` |
| 60 | `defun` |
| 61 | `defmethod` |
| 62 | `defmacro` |
| 63 | `defvar` |
| 64 | `defstruct` |
| 65 | `defenum` |
| 66 | `module` |
| 67 | `use` |
| 68 | `load` |
| 69 | `expr` |
| 70 | `defsignature` |
| 71 | `defconstant` |
| 72 | `deftrait` |
| 73 | `deftype` |
| 74 | `impl` |
| 75 | `nil` |
| 76 | `cons` |
| 77 | `path` |
| 78 | `progn` |
| 79 | `cond` |
| 80 | `and` |
| 81 | `or` |
| 82 | `list` |
| 83 | `block` |
| 84 | `when` |
| 85 | `unless` |
| 86 | `while` |
| 87 | `let*` |
| 88 | `case` |
| 89 | `setf` |
| 90 | `as` |
| 91 | `dolist` |
| 92 | `dotimes` |
| 93 | `doiter` |
| 94 | `until` |
| 95 | `do` |
| 96 | `:upcase` |
| 97 | `:capitalize` |
| 98 | `<monomorph specializations>` |
| 99 | `:with` |
| 100 | `:for` |
| 101 | `:as` |
| 102 | `:repeat` |
| 103 | `:initially` |
| 104 | `:finally` |
| 105 | `:in` |
| 106 | `:across` |
| 107 | `:on` |
| 108 | `:from` |
| 109 | `:downfrom` |
| 110 | `:upfrom` |
| 111 | `:to` |
| 112 | `:below` |
| 113 | `:downto` |
| 114 | `int-any-width` |
| 115 | `float-any-width` |
| 116 | `i32` |
| 117 | `f64` |
| 118 | `f32` |
| 119 | `:above` |
| 120 | `:by` |
| 121 | `:then` |
| 122 | `:do` |
| 123 | `:doing` |
| 124 | `:collect` |
| 125 | `:collecting` |
| 126 | `:append` |
| 127 | `:appending` |
| 128 | `:sum` |
| 129 | `:summing` |
| 130 | `:count` |
| 131 | `:counting` |
| 132 | `:maximize` |
| 133 | `:maximizing` |
| 134 | `:minimize` |
| 135 | `:minimizing` |
| 136 | `:always` |
| 137 | `:never` |
| 138 | `:thereis` |
| 139 | `:while` |
| 140 | `:until` |
| 141 | `:return` |
| 142 | `:when` |
| 143 | `:if` |
| 144 | `:unless` |
| 145 | `:else` |
| 146 | `:into` |
| 147 | `i8` |
| 148 | `i16` |
| 149 | `u8` |
| 150 | `u16` |
| 151 | `u32` |
| 152 | `return-from` |
| 153 | `in-module` |
| 154 | `import` |
| 155 | `shadowing-import` |
| 156 | `defparameter` |
| 157 | `:named` |
| 158 | `trace` |
| 159 | `untrace` |
| 160 | `step` |
| 161 | `disassemble-fn` |
| 162 | `defffi` |
| 163 | `go` |
| 164 | `select` |
| 165 | `untag-int` |
| 166 | `tag-int` |
| 167 | `some-of` |
| 168 | `pat-some` |
| 169 | `box-option` |
| 170 | `else` |
| 171 | `spawn` |
| 172 | `tag` |
| 173 | `thread` |
| 174 | `spawn-thread` |
| 175 | `task` |
| 176 | `&body` |
| 177 | `pat-when` |
| 178 | `not` |
| 179 | `unsafe` |
| 180 | `def-c-struct` |
| 181 | `self` |
| 182 | `test` |
| 183 | `ptr` |
| 184 | `recv` |
| 185 | `send` |
| 186 | `:type` |
| 187 | `:where` |
| 188 | `:or` |
| 189 | `:constructor` |
| 190 | `:copier` |
| 191 | `:include` |
| 192 | `:conc-name` |
| 193 | `:predicate` |
| 194 | `:initial-offset` |
| 195 | `:library` |
| 196 | `:prefix` |
| 197 | `:per-line-prefix` |
| 198 | `:suffix` |
| 199 | `:downcase` |
| 200 | `:linear` |
| 201 | `:fill` |
| 202 | `:miser` |
| 203 | `:mandatory` |
| 204 | `:block` |
| 205 | `:current` |
| 206 | `:line` |
| 207 | `:section` |
| 208 | `:line-relative` |
| 209 | `:section-relative` |
| 210 | `intern` |
| 211 | `_` |
| 212 | `incf` |
| 213 | `decf` |
| 214 | `rotatef` |
| 215 | `shiftf` |
| 216 | `macrolet` |
| 217 | `symbol-macrolet` |
| 218 | `try-as` |
| 219 | `compile` |
| 220 | `quasiquote` |
| 221 | `format` |
| 222 | `print` |
| 223 | `println` |
| 224 | `eprint` |
| 225 | `eprintln` |
| 226 | `source-file` |
| 227 | `pprint` |
| 228 | `pprint-fill` |
| 229 | `pprint-linear` |
| 230 | `pprint-tabular` |
| 231 | `pprint-logical-block` |
| 232 | `c-alloc` |
| 233 | `c-ref` |
| 234 | `c-deref` |
| 235 | `make-instance` |
| 236 | `documentation` |
| 237 | `disassemble` |
| 238 | `ed` |
| 239 | `%internal` |
| 240 | `attribute` |
| 241 | `apply-indirect` |
| 242 | `cellvar` |
| 243 | `cellset` |
| 244 | `global-init` |
| 245 | `hashtable-op` |
| 246 | `llvm-op` |
| 247 | `suspend` |
| 248 | `vector-op` |
| 249 | `handle` |
| 250 | `raw-word` |
| 251 | `rational` |
| 252 | `random-state` |
| 253 | `sexpr` |
| 254 | `struct` |
| 255 | `enum` |
| 256 | `dyn` |
| 257 | `scope` |
| 258 | `vector` |
| 259 | `hashtable` |
| 260 | `niche` |
| 261 | `none` |
| 262 | `%type-apply` |
| 263 | `%type-member` |
| 264 | `%value-member` |
| 265 | `shadow` |
| 266 | `type-named` |
| 267 | `type-fn` |
| 268 | `type-dyn` |
| 269 | `type-ptr` |
| 270 | `type-cstruct` |
| 271 | `type-param` |
| 272 | `type-var` |
| 273 | `type-unit` |
| 274 | `type-never` |
| 275 | `type-ref` |
| 276 | `type-value` |
| 277 | `Option` |
| 278 | `Result` |
| 279 | `Sexpr` |
| 280 | `Vector` |
| 281 | `HashTable` |
| 282 | `Scope` |
| 283 | `Chan` |
| 284 | `Task` |
| 285 | `Thread` |
| 286 | `Array` |
| 287 | `string` |
| 288 | `symbol` |
| 289 | `ParseIntError` |
| 290 | `ParseFloatError` |
| 291 | `ReadError` |
| 292 | `EvalError` |
| 293 | `FileError` |
| 294 | `NetError` |
| 295 | `llvm-module` |
| 296 | `llvm-function` |
| 297 | `llvm-builder` |
| 298 | `llvm-basic-block` |
| 299 | `llvm-value` |
| 300 | `c-long` |
| 301 | `c-ulong` |
| 302 | `!` |
| 303 | `Self` |
| 304 | `*` |
| 305 | `+` |
| 306 | `-` |
| 307 | `/` |
| 308 | `/=` |
| 309 | `<` |
| 310 | `<=` |
| 311 | `>` |
| 312 | `>=` |
| 313 | `acos` |
| 314 | `acosh` |
| 315 | `add-coroutine-function` |
| 316 | `add-function` |
| 317 | `add-function-with-env` |
| 318 | `alloca-args` |
| 319 | `alphap` |
| 320 | `append` |
| 321 | `append-block` |
| 322 | `ash` |
| 323 | `asin` |
| 324 | `asinh` |
| 325 | `atan` |
| 326 | `atanh` |
| 327 | `available-parallelism` |
| 328 | `block-terminated?` |
| 329 | `bucket-count` |
| 330 | `bucket-delete` |
| 331 | `bucket-key` |
| 332 | `bucket-put` |
| 333 | `bucket-value` |
| 334 | `build-add` |
| 335 | `build-and` |
| 336 | `build-ashr` |
| 337 | `build-br` |
| 338 | `build-call` |
| 339 | `build-call-with-env` |
| 340 | `build-cond-br` |
| 341 | `build-fixed-binop` |
| 342 | `build-fixed-div-check` |
| 343 | `build-fixed-unary` |
| 344 | `build-float-binop` |
| 345 | `build-float-unary` |
| 346 | `build-fn-address` |
| 347 | `build-free` |
| 348 | `build-icmp-eq` |
| 349 | `build-icmp-ge` |
| 350 | `build-icmp-gt` |
| 351 | `build-icmp-le` |
| 352 | `build-icmp-lt` |
| 353 | `build-icmp-ne` |
| 354 | `build-int-to-ptr` |
| 355 | `build-lshr` |
| 356 | `build-make-closure` |
| 357 | `build-malloc` |
| 358 | `build-mul` |
| 359 | `build-or` |
| 360 | `build-ptr-to-int` |
| 361 | `build-ret` |
| 362 | `build-sadd-overflow` |
| 363 | `build-select` |
| 364 | `build-shl` |
| 365 | `build-slot-ptr` |
| 366 | `build-smul-overflow` |
| 367 | `build-ssub-overflow` |
| 368 | `build-sub` |
| 369 | `build-xor` |
| 370 | `cap` |
| 371 | `ceiling` |
| 372 | `char->int` |
| 373 | `char->string` |
| 374 | `clear` |
| 375 | `clone-frames` |
| 376 | `close` |
| 377 | `const-word` |
| 378 | `coroutine-apply` |
| 379 | `coroutine-begin` |
| 380 | `coroutine-call` |
| 381 | `coroutine-call-env` |
| 382 | `coroutine-dyn-call` |
| 383 | `coroutine-end` |
| 384 | `coroutine-suspend` |
| 385 | `coroutine-unwind` |
| 386 | `cos` |
| 387 | `cosh` |
| 388 | `count` |
| 389 | `create` |
| 390 | `current-id` |
| 391 | `c-long-bits` |
| 392 | `denominator` |
| 393 | `digitp` |
| 394 | `downcase` |
| 395 | `entries` |
| 396 | `entry-word` |
| 397 | `eq` |
| 398 | `eql` |
| 399 | `equal` |
| 400 | `equalp` |
| 401 | `exp` |
| 402 | `expt` |
| 403 | `float->f32` |
| 404 | `float->f64` |
| 405 | `float->int` |
| 406 | `float->ratio` |
| 407 | `floor` |
| 408 | `frame-clear-handler` |
| 409 | `frame-set-handler` |
| 410 | `frame-slot` |
| 411 | `frame-slot-rooted` |
| 412 | `function-param` |
| 413 | `get` |
| 414 | `get-function` |
| 415 | `int->c-long` |
| 416 | `int->c-ulong` |
| 417 | `int->char` |
| 418 | `int->float` |
| 419 | `int->i16` |
| 420 | `int->i32` |
| 421 | `int->i8` |
| 422 | `int->int` |
| 423 | `int->ratio` |
| 424 | `int->u16` |
| 425 | `int->u32` |
| 426 | `int->u8` |
| 427 | `integer-length` |
| 428 | `join` |
| 429 | `keys` |
| 430 | `len` |
| 431 | `length` |
| 432 | `load-arg` |
| 433 | `load-env` |
| 434 | `load-raw` |
| 435 | `load-type` |
| 436 | `loop-safepoint` |
| 437 | `log` |
| 438 | `logand` |
| 439 | `logbitp` |
| 440 | `logcount` |
| 441 | `logior` |
| 442 | `lognot` |
| 443 | `logtest` |
| 444 | `logxor` |
| 445 | `lt` |
| 446 | `max` |
| 447 | `min` |
| 448 | `mod` |
| 449 | `new` |
| 450 | `numerator` |
| 451 | `pop` |
| 452 | `pop-frame` |
| 453 | `position-at-end` |
| 454 | `push` |
| 455 | `push-frame` |
| 456 | `ratio->float` |
| 457 | `ratio->int` |
| 458 | `ratio->rational` |
| 459 | `rational->float` |
| 460 | `rational->int` |
| 461 | `rational->ratio` |
| 462 | `ref` |
| 463 | `rem` |
| 464 | `round` |
| 465 | `scratch-slot` |
| 466 | `sin` |
| 467 | `sinh` |
| 468 | `sqrt` |
| 469 | `store-arg` |
| 470 | `substring` |
| 471 | `tan` |
| 472 | `tanh` |
| 473 | `to-string` |
| 474 | `truncate` |
| 475 | `try-float->f32` |
| 476 | `try-float->f64` |
| 477 | `try-int->char` |
| 478 | `try-int->i16` |
| 479 | `try-int->i32` |
| 480 | `try-int->i8` |
| 481 | `try-int->u16` |
| 482 | `try-int->u32` |
| 483 | `try-int->u8` |
| 484 | `try-rational->int` |
| 485 | `upcase` |
| 486 | `values` |
| 487 | `verify` |
| 488 | `wait` |
| 489 | `compile-file` |
| 490 | `dump` |
| 491 | `dribble-start` |
| 492 | `dribble-stop` |
| 493 | `ed-open` |
| 494 | `file-create-directories` |
| 495 | `file-delete` |
| 496 | `file-directory-p` |
| 497 | `file-exists-p` |
| 498 | `file-list-directory` |
| 499 | `file-modified-date` |
| 500 | `file-owner-name` |
| 501 | `file-rename` |
| 502 | `file-truename` |
| 503 | `make-random-state-fresh` |
| 504 | `net-accept` |
| 505 | `net-buffered-p` |
| 506 | `net-connect-begin` |
| 507 | `net-connect-finish` |
| 508 | `net-fill` |
| 509 | `net-flush` |
| 510 | `net-listen` |
| 511 | `net-local-address` |
| 512 | `net-peer-address` |
| 513 | `net-peer-subject` |
| 514 | `net-pop-byte` |
| 515 | `net-pop-char` |
| 516 | `net-push-byte` |
| 517 | `net-push-string` |
| 518 | `net-resolve-begin` |
| 519 | `net-resolve-finish` |
| 520 | `net-server-name` |
| 521 | `net-set-keepalive` |
| 522 | `net-set-keepalive-period` |
| 523 | `net-set-nodelay` |
| 524 | `net-shutdown-write` |
| 525 | `net-socket-error` |
| 526 | `net-tls-add-certificate` |
| 527 | `net-tls-handshake` |
| 528 | `net-tls-listen` |
| 529 | `net-tls-start` |
| 530 | `net-udp-bind` |
| 531 | `net-udp-last-sender` |
| 532 | `net-udp-recv` |
| 533 | `net-udp-send-to` |
| 534 | `net-unix-connect` |
| 535 | `net-unix-listen` |
| 536 | `net-wait` |
| 537 | `net-wait-for` |
| 538 | `random-state-copy` |
| 539 | `random-state-next` |
| 540 | `read-datum-at` |
| 541 | `stream-at-line-start` |
| 542 | `stream-close` |
| 543 | `stream-describe` |
| 544 | `stream-finish-output` |
| 545 | `stream-input-p` |
| 546 | `stream-listen` |
| 547 | `stream-open-file` |
| 548 | `stream-open-p` |
| 549 | `stream-output-p` |
| 550 | `stream-position` |
| 551 | `stream-read-byte` |
| 552 | `stream-read-char` |
| 553 | `stream-stderr` |
| 554 | `stream-stdin` |
| 555 | `stream-stdout` |
| 556 | `stream-string-input` |
| 557 | `stream-string-output` |
| 558 | `stream-take-output-string` |
| 559 | `stream-unread-char` |
| 560 | `stream-write-byte` |
| 561 | `stream-write-string` |
| 562 | `command-line-args` |
| 563 | `getenv` |
| 564 | `home-directory` |
| 565 | `lisp-implementation-version` |
| 566 | `machine-instance` |
| 567 | `machine-type` |
| 568 | `machine-version` |
| 569 | `software-type` |
| 570 | `software-version` |
| 571 | `eval` |
| 572 | `macroexpand` |
| 573 | `macroexpand-1` |
| 574 | `pprint-indent` |
| 575 | `pprint-list-exhausted` |
| 576 | `pprint-newline` |
| 577 | `pprint-pop` |
| 578 | `pprint-tab` |
| 579 | `parse-float` |
| 580 | `exit` |
| 581 | `seed-random-state` |
| 582 | `get-dispatch-macro-character` |
| 583 | `get-macro-character` |
| 584 | `read` |
| 585 | `set-dispatch-macro-character` |
| 586 | `set-macro-character` |
| 587 | `heap-info` |
| 588 | `sexpr-atom` |
| 589 | `sexpr-bool` |
| 590 | `sexpr-car` |
| 591 | `sexpr-cdr` |
| 592 | `sexpr-char` |
| 593 | `sexpr-cons` |
| 594 | `sexpr-consp` |
| 595 | `sexpr-f32` |
| 596 | `sexpr-f64` |
| 597 | `sexpr-i16` |
| 598 | `sexpr-i32` |
| 599 | `sexpr-i8` |
| 600 | `sexpr-int` |
| 601 | `sexpr-null` |
| 602 | `sexpr-str` |
| 603 | `sexpr-sym-name` |
| 604 | `sexpr-symp` |
| 605 | `sexpr-u16` |
| 606 | `sexpr-u32` |
| 607 | `sexpr-u8` |
| 608 | `keywordp` |
| 609 | `symbol->string` |
| 610 | `sleep` |
| 611 | `yield` |
| 612 | `get-internal-real-time` |
| 613 | `get-internal-run-time` |
| 614 | `get-universal-time` |
| 615 | `timezone-daylight-p` |
| 616 | `timezone-offset-seconds` |
| 617 | `format-rt` |
| 618 | `print-rt` |
| 619 | `println-rt` |
| 620 | `pprint-rt` |
| 621 | `pprint-block-start-rt` |
| 622 | `pprint-block-end-rt` |
| 623 | ` c-arena-open` |
| 624 | ` c-arena-close` |
| 625 | ` c-alloc` |
| 626 | ` c-ptr-check` |
| 627 | ` c-index` |
| 628 | ` c-offset` |
| 629 | ` c-load` |
| 630 | ` c-store` |
| 631 | ` ffi-callback-address` |
| 632 | `pat-dyn` |
| 633 | `hash-function` |
| 634 | `test-function` |
| 635 | `sxhash-eq` |
| 636 | `sxhash-eql` |
| 637 | `sxhash-equalp` |
| 638 | `&environment` |
| 639 | `define-symbol-macro` |
| 640 | `format-out` |
| 641 | ` fmt-open` |
| 642 | ` fmt-text` |
| 643 | ` fmt-fresh-line` |
| 644 | ` fmt-a` |
| 645 | ` fmt-string` |
| 646 | ` fmt-number` |
| 647 | ` fmt-value` |
| 648 | ` fmt-d` |
| 649 | ` fmt-integer` |
| 650 | ` fmt-integer-any` |
| 651 | ` fmt-words` |
| 652 | ` fmt-plural` |
| 653 | ` fmt-char` |
| 654 | ` fmt-fixed` |
| 655 | ` fmt-exponent` |
| 656 | ` fmt-general` |
| 657 | ` fmt-dollars` |
| 658 | ` fmt-tab` |
| 659 | ` fmt-newline` |
| 660 | ` fmt-indent` |
| 661 | ` fmt-case-open` |
| 662 | ` fmt-case-close` |
| 663 | ` fmt-just-open` |
| 664 | ` fmt-just-segment` |
| 665 | ` fmt-just-close` |
| 666 | ` fmt-block-open` |
| 667 | ` fmt-block-close` |
| 668 | ` fmt-result` |
| 669 | ` fmt-emit` |
| 670 | ` fmt-first` |
| 671 | ` fmt-rest` |
| 672 | ` fmt-empty` |
| 673 | ` fmt-length` |
| 674 | ` fmt-goto` |
| 675 | ` fmt-as-list` |
| 676 | ` fmt-param` |
| 677 | ` fmt-select` |
| 678 | ` fmt-bool` |
| 679 | `volatile-read-u8` |
| 680 | `volatile-read-u16` |
| 681 | `volatile-read-u32` |
| 682 | `volatile-write-u8` |
| 683 | `volatile-write-u16` |
| 684 | `volatile-write-u32` |
| 685 | ` c-volatile-load` |
| 686 | ` c-volatile-store` |
| 687 | `enable-interrupt` |
| 688 | `disable-interrupt` |

## 組み込み型 (typelisp_mem::BUILTIN_TYPES)

| 番号 | 型 |
|---|---|
| 0 | `std::option::Option` |
| 1 | `std::result::Result` |
| 2 | `std::vec::Vector` |
| 3 | `std::sexpr::cons-cell` |
| 4 | `std::collections::HashTable` |
| 5 | `std::collections::Scope` |
| 6 | `std::collections::scope-frame` |
| 7 | `std::reader::ReadError` |
| 8 | `std::fs::FileError` |
| 9 | `std::num::ParseIntError` |
| 10 | `std::num::ParseFloatError` |
| 11 | `std::time::universal-time` |
| 12 | `std::time::internal-time` |
| 13 | `std::repl::heap-info` |
| 14 | `std::net::NetError` |
| 15 | `std::sexpr::Sexpr` |
| 16 | `std::option::Option<std::sexpr::Sexpr>` |
| 17 | `std::vec::Vector<std::option::Option<std::sexpr::Sexpr>>` |
| 18 | `std::array::Array<std::option::Option<std::sexpr::Sexpr>>` |
| 19 | `std::%internal::format-out` |

## 実行時が組み立てる値の型

| 表 | 関数 | 型 |
|---|---|---|
| typelisp_rt::sys_builtin::RESULT_TYPES | `parse-float` | `std::result::Result<std::primitive::f64,std::num::ParseFloatError>` |
| typelisp_rt::sys_builtin::RESULT_TYPES | `machine-version` | `std::option::Option<std::primitive::string>` |
| typelisp_rt::sys_builtin::RESULT_TYPES | `timezone-offset-seconds` | `std::option::Option<std::primitive::int>` |
| typelisp_rt::sys_builtin::RESULT_TYPES | `timezone-daylight-p` | `std::option::Option<std::primitive::bool>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-close` | `std::result::Result<(),std::fs::FileError>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-input-p` | `std::result::Result<std::primitive::bool,std::fs::FileError>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-output-p` | `std::result::Result<std::primitive::bool,std::fs::FileError>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-listen` | `std::result::Result<std::primitive::bool,std::fs::FileError>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-position` | `std::result::Result<std::primitive::int,std::fs::FileError>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-at-line-start` | `std::result::Result<std::primitive::bool,std::fs::FileError>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-read-char` | `std::result::Result<std::option::Option<std::primitive::char>,std::fs::FileError>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-unread-char` | `std::result::Result<(),std::fs::FileError>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-read-byte` | `std::result::Result<std::option::Option<std::primitive::int>,std::fs::FileError>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-write-byte` | `std::result::Result<(),std::fs::FileError>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-write-string` | `std::result::Result<(),std::fs::FileError>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-finish-output` | `std::result::Result<(),std::fs::FileError>` |
| typelisp_rt::stream_builtin::RESULT_TYPES | `stream-take-output-string` | `std::result::Result<std::primitive::string,std::fs::FileError>` |
| typelisp_rt::stream_builtin::INNER_TYPES | `stream-read-char` | `std::option::Option<std::primitive::char>` |
| typelisp_rt::stream_builtin::INNER_TYPES | `stream-read-byte` | `std::option::Option<std::primitive::int>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-resolve-begin` | `std::result::Result<std::primitive::i32,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-resolve-finish` | `std::result::Result<std::option::Option<std::vec::Vector<std::primitive::string>>,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-connect-begin` | `std::result::Result<std::primitive::i32,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-connect-finish` | `std::result::Result<(),std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-listen` | `std::result::Result<std::primitive::i32,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-tls-listen` | `std::result::Result<std::primitive::i32,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-unix-connect` | `std::result::Result<std::primitive::i32,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-unix-listen` | `std::result::Result<std::primitive::i32,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-accept` | `std::result::Result<std::option::Option<std::primitive::i32>,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-fill` | `std::result::Result<std::option::Option<std::primitive::int>,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-pop-byte` | `std::result::Result<std::option::Option<std::primitive::int>,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-pop-char` | `std::result::Result<std::option::Option<std::primitive::char>,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-buffered-p` | `std::result::Result<std::primitive::bool,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-push-string` | `std::result::Result<(),std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-push-byte` | `std::result::Result<(),std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-flush` | `std::result::Result<std::option::Option<std::primitive::int>,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-socket-error` | `std::result::Result<std::option::Option<std::primitive::string>,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-peer-subject` | `std::result::Result<std::option::Option<std::primitive::string>,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-server-name` | `std::result::Result<std::option::Option<std::primitive::string>,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-set-nodelay` | `std::result::Result<(),std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-set-keepalive` | `std::result::Result<(),std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-set-keepalive-period` | `std::result::Result<(),std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-tls-add-certificate` | `std::result::Result<(),std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-shutdown-write` | `std::result::Result<(),std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-local-address` | `std::result::Result<std::primitive::string,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-peer-address` | `std::result::Result<std::primitive::string,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-tls-start` | `std::result::Result<(),std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-tls-handshake` | `std::result::Result<std::option::Option<std::primitive::int>,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-udp-bind` | `std::result::Result<std::primitive::i32,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-udp-send-to` | `std::result::Result<std::primitive::bool,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-udp-recv` | `std::result::Result<std::option::Option<std::vec::Vector<std::primitive::int>>,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::RESULT_TYPES | `net-udp-last-sender` | `std::result::Result<std::primitive::string,std::net::NetError>` |
| typelisp_rt_posix::net_builtin::INNER_TYPES | `net-resolve-finish` | `std::option::Option<std::vec::Vector<std::primitive::string>>` |
| typelisp_rt_posix::net_builtin::INNER_TYPES | `net-tls-handshake` | `std::option::Option<std::primitive::int>` |
| typelisp_rt_posix::net_builtin::INNER_TYPES | `net-udp-recv` | `std::option::Option<std::vec::Vector<std::primitive::int>>` |
| typelisp_rt_posix::net_builtin::INNER_TYPES | `net-accept` | `std::option::Option<std::primitive::i32>` |
| typelisp_rt_posix::net_builtin::INNER_TYPES | `net-fill` | `std::option::Option<std::primitive::int>` |
| typelisp_rt_posix::net_builtin::INNER_TYPES | `net-flush` | `std::option::Option<std::primitive::int>` |
| typelisp_rt_posix::net_builtin::INNER_TYPES | `net-socket-error` | `std::option::Option<std::primitive::string>` |
| typelisp_rt_posix::net_builtin::INNER_TYPES | `net-peer-subject` | `std::option::Option<std::primitive::string>` |
| typelisp_rt_posix::net_builtin::INNER_TYPES | `net-server-name` | `std::option::Option<std::primitive::string>` |
| typelisp_rt_posix::net_builtin::INNER_TYPES | `net-pop-byte` | `std::option::Option<std::primitive::int>` |
| typelisp_rt_posix::net_builtin::INNER_TYPES | `net-pop-char` | `std::option::Option<std::primitive::char>` |
| typelisp_rt_posix::sys::RESULT_TYPES | `machine-instance` | `std::option::Option<std::primitive::string>` |
| typelisp_rt_posix::sys::RESULT_TYPES | `software-version` | `std::option::Option<std::primitive::string>` |
| typelisp_rt_posix::sys::RESULT_TYPES | `ed-open` | `std::result::Result<(),std::fs::FileError>` |
| typelisp_rt_posix::sys::RESULT_TYPES | `file-owner-name` | `std::result::Result<std::option::Option<std::primitive::string>,std::fs::FileError>` |
| typelisp_rt_posix::sys::INNER_TYPES | `file-owner-name` | `std::option::Option<std::primitive::string>` |
| typelisp_rt_sys::fs_builtin::RESULT_TYPES | `stream-open-file` | `std::result::Result<std::primitive::i32,std::fs::FileError>` |
| typelisp_rt_sys::fs_builtin::RESULT_TYPES | `file-delete` | `std::result::Result<(),std::fs::FileError>` |
| typelisp_rt_sys::fs_builtin::RESULT_TYPES | `file-truename` | `std::result::Result<std::primitive::string,std::fs::FileError>` |
| typelisp_rt_sys::fs_builtin::RESULT_TYPES | `file-modified-date` | `std::result::Result<std::time::universal-time,std::fs::FileError>` |
| typelisp_rt_sys::fs_builtin::RESULT_TYPES | `file-list-directory` | `std::result::Result<std::vec::Vector<std::primitive::string>,std::fs::FileError>` |
| typelisp_rt_sys::fs_builtin::RESULT_TYPES | `file-create-directories` | `std::result::Result<(),std::fs::FileError>` |
| typelisp_rt_sys::fs_builtin::RESULT_TYPES | `file-rename` | `std::result::Result<(),std::fs::FileError>` |
| typelisp_rt_sys::fs_builtin::INNER_TYPES | `file-list-directory` | `std::vec::Vector<std::primitive::string>` |
| typelisp_rt_sys::env::RESULT_TYPES | `command-line-args` | `std::vec::Vector<std::primitive::string>` |
| typelisp_rt_sys::env::RESULT_TYPES | `getenv` | `std::option::Option<std::primitive::string>` |
| typelisp_rt_sys::env::RESULT_TYPES | `home-directory` | `std::option::Option<std::primitive::string>` |
| typelisp_rt_sys::dribble::RESULT_TYPES | `dribble-start` | `std::result::Result<(),std::fs::FileError>` |
| typelisp_rt_sys::dribble::RESULT_TYPES | `dribble-stop` | `std::result::Result<(),std::fs::FileError>` |
| readtable::READER_MACRO_OPTION_TYPE | | `std::option::Option<(fn (std::io::string-input-stream,std::primitive::char) std::option::Option<std::sexpr::Sexpr>)>` |

## 型ごとの表現 (Repr)

| 型 | 型キー | Repr | field_kind | binding_kind |
|---|---|---|---|---|
| `!` | `!` | none | 0 | 0 |
| `%core::llvm-basic-block` | `%core::llvm-basic-block` | handle | 1 | 0 |
| `%core::llvm-builder` | `%core::llvm-builder` | handle | 1 | 0 |
| `%core::llvm-function` | `%core::llvm-function` | handle | 1 | 0 |
| `%core::llvm-module` | `%core::llvm-module` | handle | 1 | 0 |
| `%core::llvm-value` | `%core::llvm-value` | handle | 1 | 0 |
| `()` | `()` | unit | 100 | 0 |
| `(fn (K K) bool)` | `(fn (K,K) std::primitive::bool)` | fn | 6 | 2 |
| `(fn (K) int)` | `(fn (K) std::primitive::int)` | fn | 6 | 2 |
| `(fn (std::io::string-input-stream char) std::option::Option<std::sexpr::Sexpr>)` | `(fn (std::io::string-input-stream,std::primitive::char) std::option::Option<std::sexpr::Sexpr>)` | fn | 6 | 2 |
| `E` | `E` | none | 0 | 0 |
| `K` | `K` | none | 0 | 0 |
| `T` | `T` | none | 0 | 0 |
| `V` | `V` | none | 0 | 0 |
| `bool` | `std::primitive::bool` | bool | 4 | 0 |
| `c-long` | `std::primitive::c-long` | raw-word | 0 | 0 |
| `c-ulong` | `std::primitive::c-ulong` | raw-word | 0 | 0 |
| `char` | `std::primitive::char` | char | 3 | 0 |
| `f32` | `std::primitive::f32` | f32 | 11 | 0 |
| `f64` | `std::primitive::f64` | f64 | 2 | 0 |
| `i16` | `std::primitive::i16` | i16 | 1 | 0 |
| `i32` | `std::primitive::i32` | i32 | 1 | 0 |
| `i8` | `std::primitive::i8` | i8 | 1 | 0 |
| `int` | `std::primitive::int` | int | 6 | 2 |
| `random-state` | `std::primitive::random-state` | random-state | 6 | 2 |
| `ratio` | `std::primitive::ratio` | ratio | 6 | 2 |
| `rational` | `std::primitive::rational` | rational | 6 | 2 |
| `std::array::Array<std::option::Option<std::sexpr::Sexpr>>` | `std::array::Array<std::option::Option<std::sexpr::Sexpr>>` | none | 0 | 0 |
| `std::collections::HashTable<K,V>` | `std::collections::HashTable<K,V>` | hashtable | 6 | 2 |
| `std::collections::Scope<V>` | `std::collections::Scope<V>` | scope | 6 | 2 |
| `std::eval::Environment` | `std::eval::Environment` | struct | 6 | 2 |
| `std::option::Option<(fn (std::io::string-input-stream char) std::option::Option<std::sexpr::Sexpr>)>` | `std::option::Option<(fn (std::io::string-input-stream,std::primitive::char) std::option::Option<std::sexpr::Sexpr>)>` | niche | 101 | 2 |
| `std::option::Option<T>` | `std::option::Option<T>` | enum | 6 | 2 |
| `std::option::Option<V>` | `std::option::Option<V>` | enum | 6 | 2 |
| `std::option::Option<bool>` | `std::option::Option<std::primitive::bool>` | niche | 101 | 0 |
| `std::option::Option<char>` | `std::option::Option<std::primitive::char>` | niche | 101 | 0 |
| `std::option::Option<f32>` | `std::option::Option<std::primitive::f32>` | niche | 101 | 2 |
| `std::option::Option<f64>` | `std::option::Option<std::primitive::f64>` | niche | 101 | 2 |
| `std::option::Option<i16>` | `std::option::Option<std::primitive::i16>` | niche | 101 | 0 |
| `std::option::Option<i32>` | `std::option::Option<std::primitive::i32>` | niche | 101 | 0 |
| `std::option::Option<i8>` | `std::option::Option<std::primitive::i8>` | niche | 101 | 0 |
| `std::option::Option<int>` | `std::option::Option<std::primitive::int>` | niche | 101 | 2 |
| `std::option::Option<std::sexpr::Sexpr>` | `std::option::Option<std::sexpr::Sexpr>` | niche | 101 | 2 |
| `std::option::Option<string>` | `std::option::Option<std::primitive::string>` | niche | 101 | 2 |
| `std::option::Option<u16>` | `std::option::Option<std::primitive::u16>` | niche | 101 | 0 |
| `std::option::Option<u32>` | `std::option::Option<std::primitive::u32>` | niche | 101 | 0 |
| `std::option::Option<u8>` | `std::option::Option<std::primitive::u8>` | niche | 101 | 0 |
| `std::repl::heap-info` | `std::repl::heap-info` | none | 0 | 0 |
| `std::result::Result<(),std::fs::FileError>` | `std::result::Result<(),std::fs::FileError>` | enum | 6 | 2 |
| `std::result::Result<(),std::net::NetError>` | `std::result::Result<(),std::net::NetError>` | enum | 6 | 2 |
| `std::result::Result<(std::option::Option<std::sexpr::Sexpr> . int),std::reader::ReadError>` | `std::result::Result<std::sexpr::cons-cell<std::option::Option<std::sexpr::Sexpr>,std::primitive::int>,std::reader::ReadError>` | enum | 6 | 2 |
| `std::result::Result<bool,std::fs::FileError>` | `std::result::Result<std::primitive::bool,std::fs::FileError>` | enum | 6 | 2 |
| `std::result::Result<bool,std::net::NetError>` | `std::result::Result<std::primitive::bool,std::net::NetError>` | enum | 6 | 2 |
| `std::result::Result<f64,std::num::ParseFloatError>` | `std::result::Result<std::primitive::f64,std::num::ParseFloatError>` | enum | 6 | 2 |
| `std::result::Result<i32,std::fs::FileError>` | `std::result::Result<std::primitive::i32,std::fs::FileError>` | enum | 6 | 2 |
| `std::result::Result<i32,std::net::NetError>` | `std::result::Result<std::primitive::i32,std::net::NetError>` | enum | 6 | 2 |
| `std::result::Result<int,std::fs::FileError>` | `std::result::Result<std::primitive::int,std::fs::FileError>` | enum | 6 | 2 |
| `std::result::Result<std::option::Option<char>,std::fs::FileError>` | `std::result::Result<std::option::Option<std::primitive::char>,std::fs::FileError>` | enum | 6 | 2 |
| `std::result::Result<std::option::Option<char>,std::net::NetError>` | `std::result::Result<std::option::Option<std::primitive::char>,std::net::NetError>` | enum | 6 | 2 |
| `std::result::Result<std::option::Option<i32>,std::net::NetError>` | `std::result::Result<std::option::Option<std::primitive::i32>,std::net::NetError>` | enum | 6 | 2 |
| `std::result::Result<std::option::Option<int>,std::fs::FileError>` | `std::result::Result<std::option::Option<std::primitive::int>,std::fs::FileError>` | enum | 6 | 2 |
| `std::result::Result<std::option::Option<int>,std::net::NetError>` | `std::result::Result<std::option::Option<std::primitive::int>,std::net::NetError>` | enum | 6 | 2 |
| `std::result::Result<std::option::Option<std::sexpr::Sexpr>,std::eval::EvalError>` | `std::result::Result<std::option::Option<std::sexpr::Sexpr>,std::eval::EvalError>` | enum | 6 | 2 |
| `std::result::Result<std::option::Option<std::sexpr::Sexpr>,std::reader::ReadError>` | `std::result::Result<std::option::Option<std::sexpr::Sexpr>,std::reader::ReadError>` | enum | 6 | 2 |
| `std::result::Result<std::option::Option<std::vec::Vector<int>>,std::net::NetError>` | `std::result::Result<std::option::Option<std::vec::Vector<std::primitive::int>>,std::net::NetError>` | enum | 6 | 2 |
| `std::result::Result<std::option::Option<std::vec::Vector<string>>,std::net::NetError>` | `std::result::Result<std::option::Option<std::vec::Vector<std::primitive::string>>,std::net::NetError>` | enum | 6 | 2 |
| `std::result::Result<std::option::Option<string>,std::fs::FileError>` | `std::result::Result<std::option::Option<std::primitive::string>,std::fs::FileError>` | enum | 6 | 2 |
| `std::result::Result<std::option::Option<string>,std::net::NetError>` | `std::result::Result<std::option::Option<std::primitive::string>,std::net::NetError>` | enum | 6 | 2 |
| `std::result::Result<std::time::universal-time,std::fs::FileError>` | `std::result::Result<std::time::universal-time,std::fs::FileError>` | enum | 6 | 2 |
| `std::result::Result<std::vec::Vector<string>,std::fs::FileError>` | `std::result::Result<std::vec::Vector<std::primitive::string>,std::fs::FileError>` | enum | 6 | 2 |
| `std::result::Result<string,std::fs::FileError>` | `std::result::Result<std::primitive::string,std::fs::FileError>` | enum | 6 | 2 |
| `std::result::Result<string,std::net::NetError>` | `std::result::Result<std::primitive::string,std::net::NetError>` | enum | 6 | 2 |
| `std::sexpr::Sexpr` | `std::sexpr::Sexpr` | sexpr | 6 | 2 |
| `std::sync::Chan<T>` | `std::sync::Chan<T>` | struct | 6 | 2 |
| `std::thread::Task<T>` | `std::thread::Task<T>` | struct | 6 | 2 |
| `std::thread::Thread<T>` | `std::thread::Thread<T>` | struct | 6 | 2 |
| `std::time::internal-time` | `std::time::internal-time` | none | 0 | 0 |
| `std::time::universal-time` | `std::time::universal-time` | none | 0 | 0 |
| `std::vec::Vector<(K . V)>` | `std::vec::Vector<std::sexpr::cons-cell<K,V>>` | vector | 6 | 2 |
| `std::vec::Vector<K>` | `std::vec::Vector<K>` | vector | 6 | 2 |
| `std::vec::Vector<T>` | `std::vec::Vector<T>` | vector | 6 | 2 |
| `std::vec::Vector<V>` | `std::vec::Vector<V>` | vector | 6 | 2 |
| `std::vec::Vector<int>` | `std::vec::Vector<std::primitive::int>` | vector | 6 | 2 |
| `std::vec::Vector<std::option::Option<std::sexpr::Sexpr>>` | `std::vec::Vector<std::option::Option<std::sexpr::Sexpr>>` | vector | 6 | 2 |
| `std::vec::Vector<string>` | `std::vec::Vector<std::primitive::string>` | vector | 6 | 2 |
| `string` | `std::primitive::string` | str | 6 | 2 |
| `symbol` | `std::primitive::symbol` | sym | 6 | 0 |
| `u16` | `std::primitive::u16` | u16 | 1 | 0 |
| `u32` | `std::primitive::u32` | u32 | 1 | 0 |
| `u8` | `std::primitive::u8` | u8 | 1 | 0 |

## ランタイム関数

| シンボル | LLVM の型 |
|---|---|
| `rt_atom` | `bool (word)` |
| `rt_bignum_new` | `word (ptr, u32)` |
| `rt_box_kind` | `word (word)` |
| `rt_c_alloc` | `word (ptr, u32)` |
| `rt_c_arena_close` | `word (word)` |
| `rt_c_arena_open` | `word ()` |
| `rt_c_index` | `word (word, word, word, word)` |
| `rt_c_load` | `i64 (word, word, word)` |
| `rt_c_offset` | `word (word, word)` |
| `rt_c_ptr_check` | `word (word, word)` |
| `rt_c_store` | `word (word, word, word, i64)` |
| `rt_c_volatile_load` | `u32 (word, word)` |
| `rt_c_volatile_store` | `word (word, word, u32)` |
| `rt_car` | `word (word)` |
| `rt_cdr` | `word (word)` |
| `rt_cell_get` | `word (word)` |
| `rt_cell_new` | `word (word)` |
| `rt_cell_set` | `word (word, word)` |
| `rt_char_alphap` | `bool (u32)` |
| `rt_char_digitp` | `bool (u32)` |
| `rt_char_downcase` | `u32 (u32)` |
| `rt_char_equalp` | `bool (u32, u32)` |
| `rt_char_upcase` | `u32 (u32)` |
| `rt_closure_env_get` | `word (word, word)` |
| `rt_closure_env_len` | `word (word)` |
| `rt_closure_fnptr` | `word (word)` |
| `rt_closure_new` | `word (ptr, u32)` |
| `rt_command_line_args` | `word ()` |
| `rt_cons` | `word (word, word)` |
| `rt_consp` | `bool (word)` |
| `rt_coroutine_closure_new` | `word (ptr, u32)` |
| `rt_data_field` | `word (word, word)` |
| `rt_data_new` | `word (ptr, u32)` |
| `rt_data_variant` | `word (word)` |
| `rt_dribble_start` | `word (word)` |
| `rt_dribble_stop` | `word ()` |
| `rt_drive_body` | `word (word, word, word)` |
| `rt_dyn_new` | `word (word, word)` |
| `rt_dyn_upcast` | `word (word, word)` |
| `rt_dyn_value` | `word (word)` |
| `rt_dyn_vtable` | `word (word)` |
| `rt_ed_open` | `word (word, word)` |
| `rt_eval` | `word (word)` |
| `rt_eval_init` | `word ()` |
| `rt_eval_state` | `word (word, word, word, word, word)` |
| `rt_exit` | `word (word)` |
| `rt_f32_acos` | `float (float)` |
| `rt_f32_acosh` | `float (float)` |
| `rt_f32_asin` | `float (float)` |
| `rt_f32_asinh` | `float (float)` |
| `rt_f32_atan` | `float (float)` |
| `rt_f32_atanh` | `float (float)` |
| `rt_f32_cosh` | `float (float)` |
| `rt_f32_new` | `word (float)` |
| `rt_f32_sinh` | `float (float)` |
| `rt_f32_tan` | `float (float)` |
| `rt_f32_tanh` | `float (float)` |
| `rt_f32_value` | `float (word)` |
| `rt_f64_acos` | `double (double)` |
| `rt_f64_acosh` | `double (double)` |
| `rt_f64_asin` | `double (double)` |
| `rt_f64_asinh` | `double (double)` |
| `rt_f64_atan` | `double (double)` |
| `rt_f64_atanh` | `double (double)` |
| `rt_f64_cosh` | `double (double)` |
| `rt_f64_fits_f32` | `bool (double)` |
| `rt_f64_new` | `word (double)` |
| `rt_f64_sinh` | `double (double)` |
| `rt_f64_tan` | `double (double)` |
| `rt_f64_tanh` | `double (double)` |
| `rt_f64_value` | `double (word)` |
| `rt_ffi_callback_address` | `word (word, word)` |
| `rt_ffi_callback_enter` | `word ()` |
| `rt_ffi_callback_invoke` | `word (ptr, u32)` |
| `rt_ffi_callback_leave` | `word ()` |
| `rt_ffi_callback_register` | `word (word, word, word, word)` |
| `rt_ffi_cstring_free` | `word (word)` |
| `rt_ffi_cstring_new` | `word (word)` |
| `rt_ffi_enter_native` | `word ()` |
| `rt_ffi_leave_native` | `word ()` |
| `rt_ffi_string_from_cstr` | `word (word)` |
| `rt_file_create_directories` | `word (word)` |
| `rt_file_delete` | `word (word)` |
| `rt_file_directory_p` | `bool (word)` |
| `rt_file_exists_p` | `bool (word)` |
| `rt_file_list_directory` | `word (word)` |
| `rt_file_modified_date` | `word (word)` |
| `rt_file_owner_name` | `word (word)` |
| `rt_file_rename` | `word (word, word)` |
| `rt_file_truename` | `word (word)` |
| `rt_float_to_int` | `word (double)` |
| `rt_float_to_ratio` | `word (double)` |
| `rt_fmt_as_list` | `word (ptr, u32)` |
| `rt_fmt_block_close` | `word (ptr, u32)` |
| `rt_fmt_block_open` | `word (ptr, u32)` |
| `rt_fmt_bool` | `bool (ptr, u32)` |
| `rt_fmt_case_close` | `word (ptr, u32)` |
| `rt_fmt_case_open` | `word (ptr, u32)` |
| `rt_fmt_char` | `word (ptr, u32)` |
| `rt_fmt_dollars` | `word (ptr, u32)` |
| `rt_fmt_emit` | `word (ptr, u32)` |
| `rt_fmt_empty` | `bool (ptr, u32)` |
| `rt_fmt_exponent` | `word (ptr, u32)` |
| `rt_fmt_first` | `word (ptr, u32)` |
| `rt_fmt_fixed` | `word (ptr, u32)` |
| `rt_fmt_fresh_line` | `word (ptr, u32)` |
| `rt_fmt_general` | `word (ptr, u32)` |
| `rt_fmt_goto` | `word (ptr, u32)` |
| `rt_fmt_indent` | `word (ptr, u32)` |
| `rt_fmt_integer` | `word (ptr, u32)` |
| `rt_fmt_integer_any` | `word (ptr, u32)` |
| `rt_fmt_just_close` | `word (ptr, u32)` |
| `rt_fmt_just_open` | `word (ptr, u32)` |
| `rt_fmt_just_segment` | `word (ptr, u32)` |
| `rt_fmt_length` | `i32 (ptr, u32)` |
| `rt_fmt_newline` | `word (ptr, u32)` |
| `rt_fmt_number` | `word (ptr, u32)` |
| `rt_fmt_open` | `word (ptr, u32)` |
| `rt_fmt_param` | `i32 (ptr, u32)` |
| `rt_fmt_plural` | `word (ptr, u32)` |
| `rt_fmt_rest` | `word (ptr, u32)` |
| `rt_fmt_result` | `word (ptr, u32)` |
| `rt_fmt_select` | `i32 (ptr, u32)` |
| `rt_fmt_string` | `word (ptr, u32)` |
| `rt_fmt_tab` | `word (ptr, u32)` |
| `rt_fmt_text` | `word (ptr, u32)` |
| `rt_fmt_value` | `word (ptr, u32)` |
| `rt_fmt_words` | `word (ptr, u32)` |
| `rt_frame_apply` | `word (ptr, u32)` |
| `rt_frame_call` | `word (ptr, u32)` |
| `rt_frame_call_env` | `word (ptr, u32)` |
| `rt_frame_dyn_call` | `word (ptr, u32)` |
| `rt_frame_open` | `word (word, word)` |
| `rt_get_dispatch_macro_character` | `word (u32, u32)` |
| `rt_get_internal_real_time` | `word ()` |
| `rt_get_internal_run_time` | `word ()` |
| `rt_get_macro_character` | `word (u32)` |
| `rt_get_universal_time` | `word ()` |
| `rt_getenv` | `word (word)` |
| `rt_global_get` | `word (word)` |
| `rt_global_new` | `word (word)` |
| `rt_global_set` | `word (word, word)` |
| `rt_hashtable_bucket_count` | `word (word, word)` |
| `rt_hashtable_bucket_delete` | `word (word, word, word)` |
| `rt_hashtable_bucket_key` | `word (word, word, word)` |
| `rt_hashtable_bucket_put` | `word (word, word, word, word, word)` |
| `rt_hashtable_bucket_value` | `word (word, word, word)` |
| `rt_hashtable_clear` | `word (word)` |
| `rt_hashtable_count` | `word (word)` |
| `rt_hashtable_entries` | `word (word, word)` |
| `rt_hashtable_function` | `word (word, word)` |
| `rt_hashtable_keys` | `word (word, word)` |
| `rt_hashtable_new` | `word (word, word, word)` |
| `rt_hashtable_values` | `word (word, word)` |
| `rt_heap_info` | `word ()` |
| `rt_heap_init` | `word (word, word, word)` |
| `rt_home_directory` | `word ()` |
| `rt_i16_box` | `word (i16)` |
| `rt_i16_unbox` | `i16 (word)` |
| `rt_i32_box` | `word (i32)` |
| `rt_i32_unbox` | `i32 (word)` |
| `rt_i8_box` | `word (i8)` |
| `rt_i8_unbox` | `i8 (word)` |
| `rt_int_div_fault` | `word (i64, i64)` |
| `rt_int_fits_char` | `bool (word)` |
| `rt_int_mod_fault` | `word (i64, i64)` |
| `rt_int_not_fixnum` | `word (word)` |
| `rt_int_to_char` | `u32 (i64)` |
| `rt_int_to_ratio` | `word (i64)` |
| `rt_integer_add` | `word (word, word)` |
| `rt_integer_ash` | `word (word, word)` |
| `rt_integer_cmp` | `word (word, word)` |
| `rt_integer_div` | `word (word, word)` |
| `rt_integer_fits_char` | `bool (word)` |
| `rt_integer_fits_i16` | `bool (word)` |
| `rt_integer_fits_i32` | `bool (word)` |
| `rt_integer_fits_i8` | `bool (word)` |
| `rt_integer_fits_u16` | `bool (word)` |
| `rt_integer_fits_u32` | `bool (word)` |
| `rt_integer_fits_u8` | `bool (word)` |
| `rt_integer_from_c_ulong` | `word (u64)` |
| `rt_integer_from_word` | `word (i64)` |
| `rt_integer_integer_length` | `word (word)` |
| `rt_integer_logand` | `word (word, word)` |
| `rt_integer_logbitp` | `bool (word, word)` |
| `rt_integer_logcount` | `word (word)` |
| `rt_integer_logior` | `word (word, word)` |
| `rt_integer_lognot` | `word (word)` |
| `rt_integer_logtest` | `bool (word, word)` |
| `rt_integer_logxor` | `word (word, word)` |
| `rt_integer_mod` | `word (word, word)` |
| `rt_integer_mul` | `word (word, word)` |
| `rt_integer_sub` | `word (word, word)` |
| `rt_integer_to_c_long` | `i64 (word)` |
| `rt_integer_to_c_ulong` | `u64 (word)` |
| `rt_integer_to_char` | `u32 (word)` |
| `rt_integer_to_float` | `double (word)` |
| `rt_integer_to_i16` | `i16 (word)` |
| `rt_integer_to_i32` | `i32 (word)` |
| `rt_integer_to_i8` | `i8 (word)` |
| `rt_integer_to_ratio` | `word (word)` |
| `rt_integer_to_u16` | `u16 (word)` |
| `rt_integer_to_u32` | `u32 (word)` |
| `rt_integer_to_u8` | `u8 (word)` |
| `rt_intern` | `word (word, word)` |
| `rt_intern_path` | `word (ptr, u32)` |
| `rt_intern_symbol` | `word (ptr, u32)` |
| `rt_interrupt_disable` | `word (word)` |
| `rt_interrupt_enable` | `word (word)` |
| `rt_interrupt_register` | `word (word, word)` |
| `rt_keywordp` | `bool (word)` |
| `rt_lisp_implementation_version` | `word ()` |
| `rt_list_to_path` | `word (word)` |
| `rt_list_to_sexpr_tuple` | `word (word)` |
| `rt_list_to_sexpr_vector` | `word (word)` |
| `rt_lists_to_sexpr_array` | `word (word, word)` |
| `rt_llvm_call` | `word (ptr, u32)` |
| `rt_local_frame_close` | `word (word)` |
| `rt_local_frame_open` | `word (word, word)` |
| `rt_loop_safepoint` | `word ()` |
| `rt_machine_instance` | `word ()` |
| `rt_machine_type` | `word ()` |
| `rt_machine_version` | `word ()` |
| `rt_macroexpand` | `word (word, word)` |
| `rt_macroexpand_1` | `word (word, word)` |
| `rt_main_answered` | `word (word)` |
| `rt_main_returned` | `word ()` |
| `rt_make_random_state_fresh` | `word ()` |
| `rt_match_fail` | `word ()` |
| `rt_net_accept` | `word (i32)` |
| `rt_net_buffered_p` | `word (i32)` |
| `rt_net_connect_begin` | `word (word)` |
| `rt_net_connect_finish` | `word (i32)` |
| `rt_net_fill` | `word (i32)` |
| `rt_net_flush` | `word (i32)` |
| `rt_net_listen` | `word (word, word)` |
| `rt_net_local_address` | `word (i32)` |
| `rt_net_peer_address` | `word (i32)` |
| `rt_net_peer_subject` | `word (i32)` |
| `rt_net_pop_byte` | `word (i32)` |
| `rt_net_pop_char` | `word (i32)` |
| `rt_net_push_byte` | `word (i32, word)` |
| `rt_net_push_string` | `word (i32, word)` |
| `rt_net_resolve_begin` | `word (word, word)` |
| `rt_net_resolve_finish` | `word (i32)` |
| `rt_net_server_name` | `word (i32)` |
| `rt_net_set_keepalive` | `word (i32, bool)` |
| `rt_net_set_keepalive_period` | `word (i32, word)` |
| `rt_net_set_nodelay` | `word (i32, bool)` |
| `rt_net_shutdown_write` | `word (i32)` |
| `rt_net_socket_error` | `word (i32)` |
| `rt_net_tls_add_certificate` | `word (i32, word, word, word)` |
| `rt_net_tls_handshake` | `word (i32)` |
| `rt_net_tls_listen` | `word (word, word, word, word, word)` |
| `rt_net_tls_start` | `word (i32, word, word, word, word)` |
| `rt_net_udp_bind` | `word (word, word)` |
| `rt_net_udp_last_sender` | `word (i32)` |
| `rt_net_udp_recv` | `word (i32)` |
| `rt_net_udp_send_to` | `word (i32, word, word)` |
| `rt_net_unix_connect` | `word (word)` |
| `rt_net_unix_listen` | `word (word)` |
| `rt_null` | `bool (word)` |
| `rt_panic` | `word (word)` |
| `rt_parse_float` | `word (word)` |
| `rt_path_to_list` | `word (word)` |
| `rt_pending_arg` | `word (word)` |
| `rt_pending_argc` | `word ()` |
| `rt_pending_env` | `word (word)` |
| `rt_pending_envc` | `word ()` |
| `rt_pop_sexpr_root` | `word ()` |
| `rt_pprint` | `word (word, word, word)` |
| `rt_pprint_block_end` | `word ()` |
| `rt_pprint_block_start` | `word (word, word, bool, word)` |
| `rt_pprint_indent` | `word (word, word)` |
| `rt_pprint_list_exhausted` | `bool ()` |
| `rt_pprint_newline` | `word (word)` |
| `rt_pprint_pop` | `word ()` |
| `rt_pprint_tab` | `word (word, word, word)` |
| `rt_print_enum_variant` | `word (word, word, word, word)` |
| `rt_print_field_name` | `word (word, word, word, word)` |
| `rt_print_field_template` | `word (word, word, word, word)` |
| `rt_print_global` | `word (word, word, word)` |
| `rt_print_names` | `word (word)` |
| `rt_print_object_method` | `word (word, word)` |
| `rt_push_permanent_sexpr_root` | `word (word)` |
| `rt_push_sexpr_root` | `word (word)` |
| `rt_random_state_copy` | `word (word)` |
| `rt_random_state_next` | `word (word, word)` |
| `rt_ratio_add` | `word (word, word)` |
| `rt_ratio_cmp` | `word (word, word)` |
| `rt_ratio_denominator` | `word (word)` |
| `rt_ratio_div` | `word (word, word)` |
| `rt_ratio_from_bignums` | `word (word, word)` |
| `rt_ratio_mul` | `word (word, word)` |
| `rt_ratio_numerator` | `word (word)` |
| `rt_ratio_sub` | `word (word, word)` |
| `rt_ratio_to_float` | `double (word)` |
| `rt_ratio_to_int` | `word (word)` |
| `rt_rational_add` | `word (word, word)` |
| `rt_rational_cmp` | `word (word, word)` |
| `rt_rational_denominator` | `word (word)` |
| `rt_rational_div` | `word (word, word)` |
| `rt_rational_from_ratio` | `word (word)` |
| `rt_rational_is_int` | `bool (word)` |
| `rt_rational_mul` | `word (word, word)` |
| `rt_rational_numerator` | `word (word)` |
| `rt_rational_sub` | `word (word, word)` |
| `rt_rational_to_float` | `double (word)` |
| `rt_rational_to_int` | `word (word)` |
| `rt_rational_to_ratio` | `word (word)` |
| `rt_read` | `word (word)` |
| `rt_read_datum_at` | `word (word, word, bool)` |
| `rt_root_count` | `word ()` |
| `rt_run_entry` | `word (word)` |
| `rt_run_program` | `word (word, word, word)` |
| `rt_run_program_alone` | `word (word, word, word)` |
| `rt_run_program_alone_int` | `word (word, word, word)` |
| `rt_run_program_int` | `word (word, word, word)` |
| `rt_run_program_interp` | `word (word, word, word)` |
| `rt_run_program_interp_int` | `word (word, word, word)` |
| `rt_run_program_threads` | `word (word, word, word)` |
| `rt_run_program_threads_int` | `word (word, word, word)` |
| `rt_seed_random_state` | `word (word)` |
| `rt_set_car` | `word (word, word)` |
| `rt_set_cdr` | `word (word, word)` |
| `rt_set_dispatch_macro_character` | `word (u32, u32, word)` |
| `rt_set_macro_character` | `word (u32, word)` |
| `rt_set_sexpr_root` | `word (word, word)` |
| `rt_sexpr_bool` | `bool (word)` |
| `rt_sexpr_char` | `u32 (word)` |
| `rt_sexpr_eq` | `bool (word, word)` |
| `rt_sexpr_eql` | `bool (word, word)` |
| `rt_sexpr_equal` | `bool (word, word)` |
| `rt_sexpr_equalp` | `bool (word, word)` |
| `rt_sexpr_f32` | `float (word)` |
| `rt_sexpr_f64` | `double (word)` |
| `rt_sexpr_i16` | `i16 (word)` |
| `rt_sexpr_i32` | `i32 (word)` |
| `rt_sexpr_i8` | `i8 (word)` |
| `rt_sexpr_instance_test` | `bool (word, word, word)` |
| `rt_sexpr_int` | `word (word)` |
| `rt_sexpr_str` | `word (word)` |
| `rt_sexpr_tuple_items` | `word (word)` |
| `rt_sexpr_u16` | `u16 (word)` |
| `rt_sexpr_u32` | `u32 (word)` |
| `rt_sexpr_u8` | `u8 (word)` |
| `rt_software_type` | `word ()` |
| `rt_software_version` | `word ()` |
| `rt_str_append` | `word (word, word)` |
| `rt_str_downcase` | `word (word)` |
| `rt_str_eq` | `bool (word, word)` |
| `rt_str_equalp` | `bool (word, word)` |
| `rt_str_length` | `word (word)` |
| `rt_str_lt` | `bool (word, word)` |
| `rt_str_new` | `word (ptr, u32)` |
| `rt_str_ref` | `u32 (word, word)` |
| `rt_str_substring` | `word (word, word, word)` |
| `rt_str_upcase` | `word (word)` |
| `rt_stream_at_line_start` | `word (i32)` |
| `rt_stream_close` | `word (i32)` |
| `rt_stream_describe` | `word (i32)` |
| `rt_stream_finish_output` | `word (i32)` |
| `rt_stream_input_p` | `word (i32)` |
| `rt_stream_listen` | `word (i32)` |
| `rt_stream_open_file` | `word (word, word)` |
| `rt_stream_open_p` | `bool (i32)` |
| `rt_stream_output_p` | `word (i32)` |
| `rt_stream_position` | `word (i32)` |
| `rt_stream_read_byte` | `word (i32)` |
| `rt_stream_read_char` | `word (i32)` |
| `rt_stream_stderr` | `i32 ()` |
| `rt_stream_stdin` | `i32 ()` |
| `rt_stream_stdout` | `i32 ()` |
| `rt_stream_string_input` | `i32 (word)` |
| `rt_stream_string_output` | `i32 ()` |
| `rt_stream_take_output_string` | `word (i32)` |
| `rt_stream_unread_char` | `word (i32, u32)` |
| `rt_stream_write_byte` | `word (i32, word)` |
| `rt_stream_write_string` | `word (i32, word)` |
| `rt_struct_field_count` | `word (word)` |
| `rt_struct_field_get` | `word (word, word)` |
| `rt_struct_field_set` | `word (word, word, word)` |
| `rt_struct_new` | `word (ptr, u32)` |
| `rt_struct_pop_field` | `word (word)` |
| `rt_struct_push_field` | `word (word, word)` |
| `rt_suspend_chan_cap` | `word (word)` |
| `rt_suspend_chan_close` | `word (word)` |
| `rt_suspend_chan_len` | `word (word)` |
| `rt_suspend_chan_new` | `word (word, word)` |
| `rt_suspend_chan_recv` | `word (word, word)` |
| `rt_suspend_chan_select` | `word (ptr, u32)` |
| `rt_suspend_chan_send` | `word (word, word)` |
| `rt_suspend_io` | `word (i32, word)` |
| `rt_suspend_io_for` | `word (i32, word, double)` |
| `rt_suspend_main` | `word ()` |
| `rt_suspend_sleep` | `word (double)` |
| `rt_suspend_task` | `word (word, word)` |
| `rt_suspend_thread` | `word (word, word)` |
| `rt_suspend_wait` | `word (word)` |
| `rt_suspend_yield` | `word ()` |
| `rt_sxhash_eq` | `word (word)` |
| `rt_sxhash_eql` | `word (word)` |
| `rt_sxhash_equalp` | `word (word)` |
| `rt_sym_name` | `word (word)` |
| `rt_symp` | `bool (word)` |
| `rt_thread_available_parallelism` | `word ()` |
| `rt_thread_current_id` | `word ()` |
| `rt_throw` | `word (word, word)` |
| `rt_throw_matches` | `word (word)` |
| `rt_throw_take_value` | `word ()` |
| `rt_timezone_daylight_p` | `word (word, word)` |
| `rt_timezone_offset_seconds` | `word (word, word)` |
| `rt_truncate_sexpr_roots` | `word (word)` |
| `rt_types_define` | `word (word, word, word)` |
| `rt_u16_box` | `word (u16)` |
| `rt_u16_unbox` | `u16 (word)` |
| `rt_u32_box` | `word (u32)` |
| `rt_u32_unbox` | `u32 (word)` |
| `rt_u8_box` | `word (u8)` |
| `rt_u8_unbox` | `u8 (word)` |
| `rt_unwinding` | `word ()` |
| `rt_upcast_set` | `word (word, word, word)` |
| `rt_vector_to_sexpr_tuple` | `word (word)` |
| `rt_vtable_set` | `word (word, word, word, word)` |
| `rt_wk_symbol` | `word (word)` |

## 組み込み型

### `%core::llvm-basic-block` (sum, 型引数 [])


### `%core::llvm-builder` (sum, 型引数 [])

- インスタンス `alloca-args`: `(%core::llvm-builder, int) -> %core::llvm-value`
- インスタンス `block-terminated?`: `(%core::llvm-builder) -> bool`
- インスタンス `build-add`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-and`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-ashr`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-br`: `(%core::llvm-builder, %core::llvm-basic-block) -> ()`
- インスタンス `build-call`: `(%core::llvm-builder, %core::llvm-function, %core::llvm-value, int) -> %core::llvm-value`
- インスタンス `build-call-with-env`: `(%core::llvm-builder, %core::llvm-function, %core::llvm-value, int, %core::llvm-value, int) -> %core::llvm-value`
- インスタンス `build-cond-br`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-basic-block, %core::llvm-basic-block) -> ()`
- インスタンス `build-fixed-binop`: `(%core::llvm-builder, int, string, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-fixed-div-check`: `(%core::llvm-builder, int, string, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-fixed-unary`: `(%core::llvm-builder, %core::llvm-module, int, string, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-float-binop`: `(%core::llvm-builder, %core::llvm-module, int, string, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-float-unary`: `(%core::llvm-builder, %core::llvm-module, int, string, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-fn-address`: `(%core::llvm-builder, %core::llvm-function) -> %core::llvm-value`
- インスタンス `build-free`: `(%core::llvm-builder, %core::llvm-value) -> ()`
- インスタンス `build-icmp-eq`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-icmp-ge`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-icmp-gt`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-icmp-le`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-icmp-lt`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-icmp-ne`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-int-to-ptr`: `(%core::llvm-builder, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-lshr`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-make-closure`: `(%core::llvm-builder, %core::llvm-module, %core::llvm-function, %core::llvm-value, int, int, int) -> %core::llvm-value`
- インスタンス `build-malloc`: `(%core::llvm-builder, int) -> %core::llvm-value`
- インスタンス `build-mul`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-or`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-ptr-to-int`: `(%core::llvm-builder, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-ret`: `(%core::llvm-builder, %core::llvm-value) -> ()`
- インスタンス `build-sadd-overflow`: `(%core::llvm-builder, %core::llvm-module, %core::llvm-value, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-select`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-shl`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-slot-ptr`: `(%core::llvm-builder, %core::llvm-value, int) -> %core::llvm-value`
- インスタンス `build-smul-overflow`: `(%core::llvm-builder, %core::llvm-module, %core::llvm-value, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-ssub-overflow`: `(%core::llvm-builder, %core::llvm-module, %core::llvm-value, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-sub`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `build-xor`: `(%core::llvm-builder, %core::llvm-value, %core::llvm-value) -> %core::llvm-value`
- インスタンス `const-word`: `(%core::llvm-builder, int) -> %core::llvm-value`
- インスタンス `coroutine-apply`: `(%core::llvm-builder, %core::llvm-module, %core::llvm-value, %core::llvm-value, int) -> %core::llvm-value`
- インスタンス `coroutine-begin`: `(%core::llvm-builder, %core::llvm-module, %core::llvm-function) -> ()`
- インスタンス `coroutine-call`: `(%core::llvm-builder, %core::llvm-module, %core::llvm-value, %core::llvm-value, int) -> %core::llvm-value`
- インスタンス `coroutine-call-env`: `(%core::llvm-builder, %core::llvm-module, %core::llvm-value, %core::llvm-value, int, %core::llvm-value, int) -> %core::llvm-value`
- インスタンス `coroutine-dyn-call`: `(%core::llvm-builder, %core::llvm-module, %core::llvm-value, %core::llvm-value, %core::llvm-value, int) -> %core::llvm-value`
- インスタンス `coroutine-end`: `(%core::llvm-builder, %core::llvm-module, %core::llvm-value) -> ()`
- インスタンス `coroutine-suspend`: `(%core::llvm-builder, %core::llvm-module) -> %core::llvm-value`
- インスタンス `coroutine-unwind`: `(%core::llvm-builder, %core::llvm-module) -> ()`
- 関連関数 `create`: `() -> %core::llvm-builder`
- インスタンス `entry-word`: `(%core::llvm-builder, %core::llvm-module, string, int) -> %core::llvm-value`
- インスタンス `frame-clear-handler`: `(%core::llvm-builder, %core::llvm-module) -> ()`
- インスタンス `frame-set-handler`: `(%core::llvm-builder, %core::llvm-module, %core::llvm-basic-block) -> ()`
- インスタンス `frame-slot`: `(%core::llvm-builder) -> %core::llvm-value`
- インスタンス `frame-slot-rooted`: `(%core::llvm-builder, %core::llvm-module) -> %core::llvm-value`
- インスタンス `load-arg`: `(%core::llvm-builder, %core::llvm-function, int) -> %core::llvm-value`
- インスタンス `load-env`: `(%core::llvm-builder, %core::llvm-function, int) -> %core::llvm-value`
- インスタンス `load-raw`: `(%core::llvm-builder, %core::llvm-value, int) -> %core::llvm-value`
- インスタンス `load-type`: `(%core::llvm-builder, %core::llvm-module, int) -> %core::llvm-value`
- インスタンス `loop-safepoint`: `(%core::llvm-builder, %core::llvm-module, %core::llvm-basic-block) -> ()`
- インスタンス `position-at-end`: `(%core::llvm-builder, %core::llvm-basic-block) -> ()`
- インスタンス `scratch-slot`: `(%core::llvm-builder, int) -> %core::llvm-value`
- インスタンス `store-arg`: `(%core::llvm-builder, %core::llvm-value, int, %core::llvm-value) -> ()`

### `%core::llvm-function` (sum, 型引数 [])

- インスタンス `append-block`: `(%core::llvm-function, string) -> %core::llvm-basic-block`
- インスタンス `function-param`: `(%core::llvm-function, int) -> %core::llvm-value`

### `%core::llvm-module` (sum, 型引数 [])

- インスタンス `add-coroutine-function`: `(%core::llvm-module, string) -> %core::llvm-function`
- インスタンス `add-function`: `(%core::llvm-module, string) -> %core::llvm-function`
- インスタンス `add-function-with-env`: `(%core::llvm-module, string) -> %core::llvm-function`
- インスタンス `c-long-bits`: `(%core::llvm-module) -> int`
- 関連関数 `create`: `(string) -> %core::llvm-module`
- インスタンス `get-function`: `(%core::llvm-module, string) -> %core::llvm-function`
- インスタンス `to-string`: `(%core::llvm-module) -> string`
- インスタンス `verify`: `(%core::llvm-module) -> bool`

### `%core::llvm-value` (sum, 型引数 [])


### `std::collections::HashTable` (sum, 型引数 [K, V])

- インスタンス `bucket-count`: `(std::collections::HashTable<K,V>, int) -> int`
- インスタンス `bucket-delete`: `(std::collections::HashTable<K,V>, int, int) -> ()`
- インスタンス `bucket-key`: `(std::collections::HashTable<K,V>, int, int) -> K`
- インスタンス `bucket-put`: `(std::collections::HashTable<K,V>, int, int, K, V) -> ()`
- インスタンス `bucket-value`: `(std::collections::HashTable<K,V>, int, int) -> V`
- インスタンス `clear`: `(std::collections::HashTable<K,V>) -> ()`
- インスタンス `count`: `(std::collections::HashTable<K,V>) -> int`
- インスタンス `entries`: `(std::collections::HashTable<K,V>) -> std::vec::Vector<(K . V)>`
- インスタンス `hash-function`: `(std::collections::HashTable<K,V>) -> (fn (K) int)`
- インスタンス `keys`: `(std::collections::HashTable<K,V>) -> std::vec::Vector<K>`
- 関連関数 `new`: `((fn (K) int), (fn (K K) bool)) -> std::collections::HashTable<K,V>`
- インスタンス `test-function`: `(std::collections::HashTable<K,V>) -> (fn (K K) bool)`
- インスタンス `values`: `(std::collections::HashTable<K,V>) -> std::vec::Vector<V>`

### `std::collections::Scope` (sum, 型引数 [V])

- インスタンス `clone-frames`: `(std::collections::Scope<V>) -> std::collections::Scope<V>`
- インスタンス `get`: `(std::collections::Scope<V>, string) -> std::option::Option<V>`
- 関連関数 `new`: `() -> std::collections::Scope<V>`
- インスタンス `pop-frame`: `(std::collections::Scope<V>) -> ()`
- インスタンス `push-frame`: `(std::collections::Scope<V>) -> ()`
- インスタンス `set`: `(std::collections::Scope<V>, string, V) -> ()`

### `std::eval::Environment` (struct, 型引数 [])


### `std::eval::EvalError` (sum, 型引数 [])

- 変種 0 `EvalError`: `string`

### `std::fs::FileError` (sum, 型引数 [])

- 変種 0 `FileError`: `string`

### `std::net::NetError` (sum, 型引数 [])

- 変種 0 `NetError`: `string`

### `std::num::ParseFloatError` (sum, 型引数 [])

- 変種 0 `ParseFloatError`: `string`

### `std::num::ParseIntError` (sum, 型引数 [])

- 変種 0 `ParseIntError`: `string`

### `std::option::Option` (sum, 型引数 [T])

- 変種 0 `Some`: `T`
- 変種 1 `None`: 

### `std::primitive::bool` (sum, 型引数 [])

- インスタンス `eq`: `(bool, bool) -> bool`
- インスタンス `eql`: `(bool, bool) -> bool`
- インスタンス `equal`: `(bool, bool) -> bool`
- インスタンス `equalp`: `(bool, bool) -> bool`

### `std::primitive::c-long` (sum, 型引数 [])

- インスタンス `int->c-long`: `(c-long) -> c-long`
- インスタンス `int->c-ulong`: `(c-long) -> c-ulong`
- インスタンス `int->i16`: `(c-long) -> i16`
- インスタンス `int->i32`: `(c-long) -> i32`
- インスタンス `int->i8`: `(c-long) -> i8`
- インスタンス `int->int`: `(c-long) -> int`
- インスタンス `int->u16`: `(c-long) -> u16`
- インスタンス `int->u32`: `(c-long) -> u32`
- インスタンス `int->u8`: `(c-long) -> u8`
- インスタンス `try-int->i16`: `(c-long) -> std::option::Option<i16>`
- インスタンス `try-int->i32`: `(c-long) -> std::option::Option<i32>`
- インスタンス `try-int->i8`: `(c-long) -> std::option::Option<i8>`
- インスタンス `try-int->u16`: `(c-long) -> std::option::Option<u16>`
- インスタンス `try-int->u32`: `(c-long) -> std::option::Option<u32>`
- インスタンス `try-int->u8`: `(c-long) -> std::option::Option<u8>`

### `std::primitive::c-ulong` (sum, 型引数 [])

- インスタンス `int->c-long`: `(c-ulong) -> c-long`
- インスタンス `int->c-ulong`: `(c-ulong) -> c-ulong`
- インスタンス `int->i16`: `(c-ulong) -> i16`
- インスタンス `int->i32`: `(c-ulong) -> i32`
- インスタンス `int->i8`: `(c-ulong) -> i8`
- インスタンス `int->int`: `(c-ulong) -> int`
- インスタンス `int->u16`: `(c-ulong) -> u16`
- インスタンス `int->u32`: `(c-ulong) -> u32`
- インスタンス `int->u8`: `(c-ulong) -> u8`
- インスタンス `try-int->i16`: `(c-ulong) -> std::option::Option<i16>`
- インスタンス `try-int->i32`: `(c-ulong) -> std::option::Option<i32>`
- インスタンス `try-int->i8`: `(c-ulong) -> std::option::Option<i8>`
- インスタンス `try-int->u16`: `(c-ulong) -> std::option::Option<u16>`
- インスタンス `try-int->u32`: `(c-ulong) -> std::option::Option<u32>`
- インスタンス `try-int->u8`: `(c-ulong) -> std::option::Option<u8>`

### `std::primitive::char` (sum, 型引数 [])

- インスタンス `<`: `(char, char) -> bool`
- インスタンス `<=`: `(char, char) -> bool`
- インスタンス `>`: `(char, char) -> bool`
- インスタンス `>=`: `(char, char) -> bool`
- インスタンス `alphap`: `(char) -> bool`
- インスタンス `char->int`: `(char) -> int`
- インスタンス `char->string`: `(char) -> string`
- インスタンス `digitp`: `(char) -> bool`
- インスタンス `downcase`: `(char) -> char`
- インスタンス `eq`: `(char, char) -> bool`
- インスタンス `eql`: `(char, char) -> bool`
- インスタンス `equal`: `(char, char) -> bool`
- インスタンス `equalp`: `(char, char) -> bool`
- インスタンス `lt`: `(char, char) -> bool`
- インスタンス `upcase`: `(char) -> char`

### `std::primitive::f32` (sum, 型引数 [])

- インスタンス `*`: `(f32, f32) -> f32`
- インスタンス `+`: `(f32, f32) -> f32`
- インスタンス `-`: `(f32, f32) -> f32`
- インスタンス `/`: `(f32, f32) -> f32`
- インスタンス `/=`: `(f32, f32) -> bool`
- インスタンス `<`: `(f32, f32) -> bool`
- インスタンス `<=`: `(f32, f32) -> bool`
- インスタンス `=`: `(f32, f32) -> bool`
- インスタンス `>`: `(f32, f32) -> bool`
- インスタンス `>=`: `(f32, f32) -> bool`
- インスタンス `acos`: `(f32) -> f32`
- インスタンス `acosh`: `(f32) -> f32`
- インスタンス `asin`: `(f32) -> f32`
- インスタンス `asinh`: `(f32) -> f32`
- インスタンス `atan`: `(f32) -> f32`
- インスタンス `atanh`: `(f32) -> f32`
- インスタンス `ceiling`: `(f32) -> f32`
- インスタンス `cos`: `(f32) -> f32`
- インスタンス `cosh`: `(f32) -> f32`
- インスタンス `eq`: `(f32, f32) -> bool`
- インスタンス `eql`: `(f32, f32) -> bool`
- インスタンス `equal`: `(f32, f32) -> bool`
- インスタンス `equalp`: `(f32, f32) -> bool`
- インスタンス `exp`: `(f32) -> f32`
- インスタンス `expt`: `(f32, f32) -> f32`
- インスタンス `float->f32`: `(f32) -> f32`
- インスタンス `float->f64`: `(f32) -> f64`
- インスタンス `float->int`: `(f32) -> int`
- インスタンス `float->ratio`: `(f32) -> ratio`
- インスタンス `floor`: `(f32) -> f32`
- インスタンス `log`: `(f32) -> f32`
- インスタンス `max`: `(f32, f32) -> f32`
- インスタンス `min`: `(f32, f32) -> f32`
- インスタンス `round`: `(f32) -> f32`
- インスタンス `sin`: `(f32) -> f32`
- インスタンス `sinh`: `(f32) -> f32`
- インスタンス `sqrt`: `(f32) -> f32`
- インスタンス `tan`: `(f32) -> f32`
- インスタンス `tanh`: `(f32) -> f32`
- インスタンス `truncate`: `(f32) -> f32`
- インスタンス `try-float->f32`: `(f32) -> std::option::Option<f32>`
- インスタンス `try-float->f64`: `(f32) -> std::option::Option<f64>`

### `std::primitive::f64` (sum, 型引数 [])

- インスタンス `*`: `(f64, f64) -> f64`
- インスタンス `+`: `(f64, f64) -> f64`
- インスタンス `-`: `(f64, f64) -> f64`
- インスタンス `/`: `(f64, f64) -> f64`
- インスタンス `/=`: `(f64, f64) -> bool`
- インスタンス `<`: `(f64, f64) -> bool`
- インスタンス `<=`: `(f64, f64) -> bool`
- インスタンス `=`: `(f64, f64) -> bool`
- インスタンス `>`: `(f64, f64) -> bool`
- インスタンス `>=`: `(f64, f64) -> bool`
- インスタンス `acos`: `(f64) -> f64`
- インスタンス `acosh`: `(f64) -> f64`
- インスタンス `asin`: `(f64) -> f64`
- インスタンス `asinh`: `(f64) -> f64`
- インスタンス `atan`: `(f64) -> f64`
- インスタンス `atanh`: `(f64) -> f64`
- インスタンス `ceiling`: `(f64) -> f64`
- インスタンス `cos`: `(f64) -> f64`
- インスタンス `cosh`: `(f64) -> f64`
- インスタンス `eq`: `(f64, f64) -> bool`
- インスタンス `eql`: `(f64, f64) -> bool`
- インスタンス `equal`: `(f64, f64) -> bool`
- インスタンス `equalp`: `(f64, f64) -> bool`
- インスタンス `exp`: `(f64) -> f64`
- インスタンス `expt`: `(f64, f64) -> f64`
- インスタンス `float->f32`: `(f64) -> f32`
- インスタンス `float->f64`: `(f64) -> f64`
- インスタンス `float->int`: `(f64) -> int`
- インスタンス `float->ratio`: `(f64) -> ratio`
- インスタンス `floor`: `(f64) -> f64`
- インスタンス `log`: `(f64) -> f64`
- インスタンス `max`: `(f64, f64) -> f64`
- インスタンス `min`: `(f64, f64) -> f64`
- インスタンス `round`: `(f64) -> f64`
- インスタンス `sin`: `(f64) -> f64`
- インスタンス `sinh`: `(f64) -> f64`
- インスタンス `sqrt`: `(f64) -> f64`
- インスタンス `tan`: `(f64) -> f64`
- インスタンス `tanh`: `(f64) -> f64`
- インスタンス `truncate`: `(f64) -> f64`
- インスタンス `try-float->f32`: `(f64) -> std::option::Option<f32>`
- インスタンス `try-float->f64`: `(f64) -> std::option::Option<f64>`

### `std::primitive::i16` (sum, 型引数 [])

- インスタンス `*`: `(i16, i16) -> i16`
- インスタンス `+`: `(i16, i16) -> i16`
- インスタンス `-`: `(i16, i16) -> i16`
- インスタンス `/`: `(i16, i16) -> i16`
- インスタンス `/=`: `(i16, i16) -> bool`
- インスタンス `<`: `(i16, i16) -> bool`
- インスタンス `<=`: `(i16, i16) -> bool`
- インスタンス `=`: `(i16, i16) -> bool`
- インスタンス `>`: `(i16, i16) -> bool`
- インスタンス `>=`: `(i16, i16) -> bool`
- インスタンス `ash`: `(i16, int) -> i16`
- インスタンス `eq`: `(i16, i16) -> bool`
- インスタンス `eql`: `(i16, i16) -> bool`
- インスタンス `equal`: `(i16, i16) -> bool`
- インスタンス `equalp`: `(i16, i16) -> bool`
- インスタンス `int->c-long`: `(i16) -> c-long`
- インスタンス `int->c-ulong`: `(i16) -> c-ulong`
- インスタンス `int->char`: `(i16) -> char`
- インスタンス `int->float`: `(i16) -> f64`
- インスタンス `int->i16`: `(i16) -> i16`
- インスタンス `int->i32`: `(i16) -> i32`
- インスタンス `int->i8`: `(i16) -> i8`
- インスタンス `int->int`: `(i16) -> int`
- インスタンス `int->ratio`: `(i16) -> ratio`
- インスタンス `int->u16`: `(i16) -> u16`
- インスタンス `int->u32`: `(i16) -> u32`
- インスタンス `int->u8`: `(i16) -> u8`
- インスタンス `integer-length`: `(i16) -> i16`
- インスタンス `logand`: `(i16, i16) -> i16`
- インスタンス `logbitp`: `(i16, int) -> bool`
- インスタンス `logcount`: `(i16) -> i16`
- インスタンス `logior`: `(i16, i16) -> i16`
- インスタンス `lognot`: `(i16) -> i16`
- インスタンス `logtest`: `(i16, i16) -> bool`
- インスタンス `logxor`: `(i16, i16) -> i16`
- インスタンス `max`: `(i16, i16) -> i16`
- インスタンス `min`: `(i16, i16) -> i16`
- インスタンス `mod`: `(i16, i16) -> i16`
- インスタンス `rem`: `(i16, i16) -> i16`
- インスタンス `try-int->char`: `(i16) -> std::option::Option<char>`
- インスタンス `try-int->i16`: `(i16) -> std::option::Option<i16>`
- インスタンス `try-int->i32`: `(i16) -> std::option::Option<i32>`
- インスタンス `try-int->i8`: `(i16) -> std::option::Option<i8>`
- インスタンス `try-int->u16`: `(i16) -> std::option::Option<u16>`
- インスタンス `try-int->u32`: `(i16) -> std::option::Option<u32>`
- インスタンス `try-int->u8`: `(i16) -> std::option::Option<u8>`

### `std::primitive::i32` (sum, 型引数 [])

- インスタンス `*`: `(i32, i32) -> i32`
- インスタンス `+`: `(i32, i32) -> i32`
- インスタンス `-`: `(i32, i32) -> i32`
- インスタンス `/`: `(i32, i32) -> i32`
- インスタンス `/=`: `(i32, i32) -> bool`
- インスタンス `<`: `(i32, i32) -> bool`
- インスタンス `<=`: `(i32, i32) -> bool`
- インスタンス `=`: `(i32, i32) -> bool`
- インスタンス `>`: `(i32, i32) -> bool`
- インスタンス `>=`: `(i32, i32) -> bool`
- インスタンス `ash`: `(i32, int) -> i32`
- インスタンス `eq`: `(i32, i32) -> bool`
- インスタンス `eql`: `(i32, i32) -> bool`
- インスタンス `equal`: `(i32, i32) -> bool`
- インスタンス `equalp`: `(i32, i32) -> bool`
- インスタンス `int->c-long`: `(i32) -> c-long`
- インスタンス `int->c-ulong`: `(i32) -> c-ulong`
- インスタンス `int->char`: `(i32) -> char`
- インスタンス `int->float`: `(i32) -> f64`
- インスタンス `int->i16`: `(i32) -> i16`
- インスタンス `int->i32`: `(i32) -> i32`
- インスタンス `int->i8`: `(i32) -> i8`
- インスタンス `int->int`: `(i32) -> int`
- インスタンス `int->ratio`: `(i32) -> ratio`
- インスタンス `int->u16`: `(i32) -> u16`
- インスタンス `int->u32`: `(i32) -> u32`
- インスタンス `int->u8`: `(i32) -> u8`
- インスタンス `integer-length`: `(i32) -> i32`
- インスタンス `logand`: `(i32, i32) -> i32`
- インスタンス `logbitp`: `(i32, int) -> bool`
- インスタンス `logcount`: `(i32) -> i32`
- インスタンス `logior`: `(i32, i32) -> i32`
- インスタンス `lognot`: `(i32) -> i32`
- インスタンス `logtest`: `(i32, i32) -> bool`
- インスタンス `logxor`: `(i32, i32) -> i32`
- インスタンス `max`: `(i32, i32) -> i32`
- インスタンス `min`: `(i32, i32) -> i32`
- インスタンス `mod`: `(i32, i32) -> i32`
- インスタンス `rem`: `(i32, i32) -> i32`
- インスタンス `try-int->char`: `(i32) -> std::option::Option<char>`
- インスタンス `try-int->i16`: `(i32) -> std::option::Option<i16>`
- インスタンス `try-int->i32`: `(i32) -> std::option::Option<i32>`
- インスタンス `try-int->i8`: `(i32) -> std::option::Option<i8>`
- インスタンス `try-int->u16`: `(i32) -> std::option::Option<u16>`
- インスタンス `try-int->u32`: `(i32) -> std::option::Option<u32>`
- インスタンス `try-int->u8`: `(i32) -> std::option::Option<u8>`

### `std::primitive::i8` (sum, 型引数 [])

- インスタンス `*`: `(i8, i8) -> i8`
- インスタンス `+`: `(i8, i8) -> i8`
- インスタンス `-`: `(i8, i8) -> i8`
- インスタンス `/`: `(i8, i8) -> i8`
- インスタンス `/=`: `(i8, i8) -> bool`
- インスタンス `<`: `(i8, i8) -> bool`
- インスタンス `<=`: `(i8, i8) -> bool`
- インスタンス `=`: `(i8, i8) -> bool`
- インスタンス `>`: `(i8, i8) -> bool`
- インスタンス `>=`: `(i8, i8) -> bool`
- インスタンス `ash`: `(i8, int) -> i8`
- インスタンス `eq`: `(i8, i8) -> bool`
- インスタンス `eql`: `(i8, i8) -> bool`
- インスタンス `equal`: `(i8, i8) -> bool`
- インスタンス `equalp`: `(i8, i8) -> bool`
- インスタンス `int->c-long`: `(i8) -> c-long`
- インスタンス `int->c-ulong`: `(i8) -> c-ulong`
- インスタンス `int->char`: `(i8) -> char`
- インスタンス `int->float`: `(i8) -> f64`
- インスタンス `int->i16`: `(i8) -> i16`
- インスタンス `int->i32`: `(i8) -> i32`
- インスタンス `int->i8`: `(i8) -> i8`
- インスタンス `int->int`: `(i8) -> int`
- インスタンス `int->ratio`: `(i8) -> ratio`
- インスタンス `int->u16`: `(i8) -> u16`
- インスタンス `int->u32`: `(i8) -> u32`
- インスタンス `int->u8`: `(i8) -> u8`
- インスタンス `integer-length`: `(i8) -> i8`
- インスタンス `logand`: `(i8, i8) -> i8`
- インスタンス `logbitp`: `(i8, int) -> bool`
- インスタンス `logcount`: `(i8) -> i8`
- インスタンス `logior`: `(i8, i8) -> i8`
- インスタンス `lognot`: `(i8) -> i8`
- インスタンス `logtest`: `(i8, i8) -> bool`
- インスタンス `logxor`: `(i8, i8) -> i8`
- インスタンス `max`: `(i8, i8) -> i8`
- インスタンス `min`: `(i8, i8) -> i8`
- インスタンス `mod`: `(i8, i8) -> i8`
- インスタンス `rem`: `(i8, i8) -> i8`
- インスタンス `try-int->char`: `(i8) -> std::option::Option<char>`
- インスタンス `try-int->i16`: `(i8) -> std::option::Option<i16>`
- インスタンス `try-int->i32`: `(i8) -> std::option::Option<i32>`
- インスタンス `try-int->i8`: `(i8) -> std::option::Option<i8>`
- インスタンス `try-int->u16`: `(i8) -> std::option::Option<u16>`
- インスタンス `try-int->u32`: `(i8) -> std::option::Option<u32>`
- インスタンス `try-int->u8`: `(i8) -> std::option::Option<u8>`

### `std::primitive::int` (sum, 型引数 [])

- インスタンス `*`: `(int, int) -> int`
- インスタンス `+`: `(int, int) -> int`
- インスタンス `-`: `(int, int) -> int`
- インスタンス `/`: `(int, int) -> rational`
- インスタンス `/=`: `(int, int) -> bool`
- インスタンス `<`: `(int, int) -> bool`
- インスタンス `<=`: `(int, int) -> bool`
- インスタンス `=`: `(int, int) -> bool`
- インスタンス `>`: `(int, int) -> bool`
- インスタンス `>=`: `(int, int) -> bool`
- インスタンス `ash`: `(int, int) -> int`
- インスタンス `eq`: `(int, int) -> bool`
- インスタンス `eql`: `(int, int) -> bool`
- インスタンス `equal`: `(int, int) -> bool`
- インスタンス `equalp`: `(int, int) -> bool`
- インスタンス `int->c-long`: `(int) -> c-long`
- インスタンス `int->c-ulong`: `(int) -> c-ulong`
- インスタンス `int->char`: `(int) -> char`
- インスタンス `int->float`: `(int) -> f64`
- インスタンス `int->i16`: `(int) -> i16`
- インスタンス `int->i32`: `(int) -> i32`
- インスタンス `int->i8`: `(int) -> i8`
- インスタンス `int->int`: `(int) -> int`
- インスタンス `int->ratio`: `(int) -> ratio`
- インスタンス `int->u16`: `(int) -> u16`
- インスタンス `int->u32`: `(int) -> u32`
- インスタンス `int->u8`: `(int) -> u8`
- インスタンス `integer-length`: `(int) -> int`
- インスタンス `logand`: `(int, int) -> int`
- インスタンス `logbitp`: `(int, int) -> bool`
- インスタンス `logcount`: `(int) -> int`
- インスタンス `logior`: `(int, int) -> int`
- インスタンス `lognot`: `(int) -> int`
- インスタンス `logtest`: `(int, int) -> bool`
- インスタンス `logxor`: `(int, int) -> int`
- インスタンス `max`: `(int, int) -> int`
- インスタンス `min`: `(int, int) -> int`
- インスタンス `mod`: `(int, int) -> int`
- インスタンス `truncate`: `(int, int) -> int`
- インスタンス `try-int->char`: `(int) -> std::option::Option<char>`
- インスタンス `try-int->i16`: `(int) -> std::option::Option<i16>`
- インスタンス `try-int->i32`: `(int) -> std::option::Option<i32>`
- インスタンス `try-int->i8`: `(int) -> std::option::Option<i8>`
- インスタンス `try-int->u16`: `(int) -> std::option::Option<u16>`
- インスタンス `try-int->u32`: `(int) -> std::option::Option<u32>`
- インスタンス `try-int->u8`: `(int) -> std::option::Option<u8>`

### `std::primitive::random-state` (sum, 型引数 [])


### `std::primitive::ratio` (sum, 型引数 [])

- インスタンス `*`: `(ratio, ratio) -> ratio`
- インスタンス `+`: `(ratio, ratio) -> ratio`
- インスタンス `-`: `(ratio, ratio) -> ratio`
- インスタンス `/`: `(ratio, ratio) -> ratio`
- インスタンス `/=`: `(ratio, ratio) -> bool`
- インスタンス `<`: `(ratio, ratio) -> bool`
- インスタンス `<=`: `(ratio, ratio) -> bool`
- インスタンス `=`: `(ratio, ratio) -> bool`
- インスタンス `>`: `(ratio, ratio) -> bool`
- インスタンス `>=`: `(ratio, ratio) -> bool`
- インスタンス `denominator`: `(ratio) -> int`
- インスタンス `eq`: `(ratio, ratio) -> bool`
- インスタンス `eql`: `(ratio, ratio) -> bool`
- インスタンス `equal`: `(ratio, ratio) -> bool`
- インスタンス `equalp`: `(ratio, ratio) -> bool`
- インスタンス `max`: `(ratio, ratio) -> ratio`
- インスタンス `min`: `(ratio, ratio) -> ratio`
- インスタンス `numerator`: `(ratio) -> int`
- インスタンス `ratio->float`: `(ratio) -> f64`
- インスタンス `ratio->int`: `(ratio) -> int`
- インスタンス `ratio->rational`: `(ratio) -> rational`

### `std::primitive::rational` (sum, 型引数 [])

- インスタンス `*`: `(rational, rational) -> rational`
- インスタンス `+`: `(rational, rational) -> rational`
- インスタンス `-`: `(rational, rational) -> rational`
- インスタンス `/`: `(rational, rational) -> rational`
- インスタンス `/=`: `(rational, rational) -> bool`
- インスタンス `<`: `(rational, rational) -> bool`
- インスタンス `<=`: `(rational, rational) -> bool`
- インスタンス `=`: `(rational, rational) -> bool`
- インスタンス `>`: `(rational, rational) -> bool`
- インスタンス `>=`: `(rational, rational) -> bool`
- インスタンス `denominator`: `(rational) -> int`
- インスタンス `eq`: `(rational, rational) -> bool`
- インスタンス `eql`: `(rational, rational) -> bool`
- インスタンス `equal`: `(rational, rational) -> bool`
- インスタンス `equalp`: `(rational, rational) -> bool`
- インスタンス `max`: `(rational, rational) -> rational`
- インスタンス `min`: `(rational, rational) -> rational`
- インスタンス `numerator`: `(rational) -> int`
- インスタンス `rational->float`: `(rational) -> f64`
- インスタンス `rational->int`: `(rational) -> int`
- インスタンス `rational->ratio`: `(rational) -> ratio`
- インスタンス `try-rational->int`: `(rational) -> std::option::Option<int>`

### `std::primitive::string` (sum, 型引数 [])

- インスタンス `<`: `(string, string) -> bool`
- インスタンス `<=`: `(string, string) -> bool`
- インスタンス `>`: `(string, string) -> bool`
- インスタンス `>=`: `(string, string) -> bool`
- インスタンス `append`: `(string, string) -> string`
- インスタンス `downcase`: `(string) -> string`
- インスタンス `eq`: `(string, string) -> bool`
- インスタンス `eql`: `(string, string) -> bool`
- インスタンス `equal`: `(string, string) -> bool`
- インスタンス `equalp`: `(string, string) -> bool`
- インスタンス `length`: `(string) -> int`
- インスタンス `lt`: `(string, string) -> bool`
- インスタンス `ref`: `(string, int) -> char`
- インスタンス `substring`: `(string, int, int) -> string`
- インスタンス `upcase`: `(string) -> string`

### `std::primitive::symbol` (sum, 型引数 [])

- インスタンス `eq`: `(symbol, symbol) -> bool`
- インスタンス `eql`: `(symbol, symbol) -> bool`

### `std::primitive::u16` (sum, 型引数 [])

- インスタンス `*`: `(u16, u16) -> u16`
- インスタンス `+`: `(u16, u16) -> u16`
- インスタンス `-`: `(u16, u16) -> u16`
- インスタンス `/`: `(u16, u16) -> u16`
- インスタンス `/=`: `(u16, u16) -> bool`
- インスタンス `<`: `(u16, u16) -> bool`
- インスタンス `<=`: `(u16, u16) -> bool`
- インスタンス `=`: `(u16, u16) -> bool`
- インスタンス `>`: `(u16, u16) -> bool`
- インスタンス `>=`: `(u16, u16) -> bool`
- インスタンス `ash`: `(u16, int) -> u16`
- インスタンス `eq`: `(u16, u16) -> bool`
- インスタンス `eql`: `(u16, u16) -> bool`
- インスタンス `equal`: `(u16, u16) -> bool`
- インスタンス `equalp`: `(u16, u16) -> bool`
- インスタンス `int->c-long`: `(u16) -> c-long`
- インスタンス `int->c-ulong`: `(u16) -> c-ulong`
- インスタンス `int->char`: `(u16) -> char`
- インスタンス `int->float`: `(u16) -> f64`
- インスタンス `int->i16`: `(u16) -> i16`
- インスタンス `int->i32`: `(u16) -> i32`
- インスタンス `int->i8`: `(u16) -> i8`
- インスタンス `int->int`: `(u16) -> int`
- インスタンス `int->ratio`: `(u16) -> ratio`
- インスタンス `int->u16`: `(u16) -> u16`
- インスタンス `int->u32`: `(u16) -> u32`
- インスタンス `int->u8`: `(u16) -> u8`
- インスタンス `integer-length`: `(u16) -> u16`
- インスタンス `logand`: `(u16, u16) -> u16`
- インスタンス `logbitp`: `(u16, int) -> bool`
- インスタンス `logcount`: `(u16) -> u16`
- インスタンス `logior`: `(u16, u16) -> u16`
- インスタンス `lognot`: `(u16) -> u16`
- インスタンス `logtest`: `(u16, u16) -> bool`
- インスタンス `logxor`: `(u16, u16) -> u16`
- インスタンス `max`: `(u16, u16) -> u16`
- インスタンス `min`: `(u16, u16) -> u16`
- インスタンス `mod`: `(u16, u16) -> u16`
- インスタンス `rem`: `(u16, u16) -> u16`
- インスタンス `try-int->char`: `(u16) -> std::option::Option<char>`
- インスタンス `try-int->i16`: `(u16) -> std::option::Option<i16>`
- インスタンス `try-int->i32`: `(u16) -> std::option::Option<i32>`
- インスタンス `try-int->i8`: `(u16) -> std::option::Option<i8>`
- インスタンス `try-int->u16`: `(u16) -> std::option::Option<u16>`
- インスタンス `try-int->u32`: `(u16) -> std::option::Option<u32>`
- インスタンス `try-int->u8`: `(u16) -> std::option::Option<u8>`

### `std::primitive::u32` (sum, 型引数 [])

- インスタンス `*`: `(u32, u32) -> u32`
- インスタンス `+`: `(u32, u32) -> u32`
- インスタンス `-`: `(u32, u32) -> u32`
- インスタンス `/`: `(u32, u32) -> u32`
- インスタンス `/=`: `(u32, u32) -> bool`
- インスタンス `<`: `(u32, u32) -> bool`
- インスタンス `<=`: `(u32, u32) -> bool`
- インスタンス `=`: `(u32, u32) -> bool`
- インスタンス `>`: `(u32, u32) -> bool`
- インスタンス `>=`: `(u32, u32) -> bool`
- インスタンス `ash`: `(u32, int) -> u32`
- インスタンス `eq`: `(u32, u32) -> bool`
- インスタンス `eql`: `(u32, u32) -> bool`
- インスタンス `equal`: `(u32, u32) -> bool`
- インスタンス `equalp`: `(u32, u32) -> bool`
- インスタンス `int->c-long`: `(u32) -> c-long`
- インスタンス `int->c-ulong`: `(u32) -> c-ulong`
- インスタンス `int->char`: `(u32) -> char`
- インスタンス `int->float`: `(u32) -> f64`
- インスタンス `int->i16`: `(u32) -> i16`
- インスタンス `int->i32`: `(u32) -> i32`
- インスタンス `int->i8`: `(u32) -> i8`
- インスタンス `int->int`: `(u32) -> int`
- インスタンス `int->ratio`: `(u32) -> ratio`
- インスタンス `int->u16`: `(u32) -> u16`
- インスタンス `int->u32`: `(u32) -> u32`
- インスタンス `int->u8`: `(u32) -> u8`
- インスタンス `integer-length`: `(u32) -> u32`
- インスタンス `logand`: `(u32, u32) -> u32`
- インスタンス `logbitp`: `(u32, int) -> bool`
- インスタンス `logcount`: `(u32) -> u32`
- インスタンス `logior`: `(u32, u32) -> u32`
- インスタンス `lognot`: `(u32) -> u32`
- インスタンス `logtest`: `(u32, u32) -> bool`
- インスタンス `logxor`: `(u32, u32) -> u32`
- インスタンス `max`: `(u32, u32) -> u32`
- インスタンス `min`: `(u32, u32) -> u32`
- インスタンス `mod`: `(u32, u32) -> u32`
- インスタンス `rem`: `(u32, u32) -> u32`
- インスタンス `try-int->char`: `(u32) -> std::option::Option<char>`
- インスタンス `try-int->i16`: `(u32) -> std::option::Option<i16>`
- インスタンス `try-int->i32`: `(u32) -> std::option::Option<i32>`
- インスタンス `try-int->i8`: `(u32) -> std::option::Option<i8>`
- インスタンス `try-int->u16`: `(u32) -> std::option::Option<u16>`
- インスタンス `try-int->u32`: `(u32) -> std::option::Option<u32>`
- インスタンス `try-int->u8`: `(u32) -> std::option::Option<u8>`

### `std::primitive::u8` (sum, 型引数 [])

- インスタンス `*`: `(u8, u8) -> u8`
- インスタンス `+`: `(u8, u8) -> u8`
- インスタンス `-`: `(u8, u8) -> u8`
- インスタンス `/`: `(u8, u8) -> u8`
- インスタンス `/=`: `(u8, u8) -> bool`
- インスタンス `<`: `(u8, u8) -> bool`
- インスタンス `<=`: `(u8, u8) -> bool`
- インスタンス `=`: `(u8, u8) -> bool`
- インスタンス `>`: `(u8, u8) -> bool`
- インスタンス `>=`: `(u8, u8) -> bool`
- インスタンス `ash`: `(u8, int) -> u8`
- インスタンス `eq`: `(u8, u8) -> bool`
- インスタンス `eql`: `(u8, u8) -> bool`
- インスタンス `equal`: `(u8, u8) -> bool`
- インスタンス `equalp`: `(u8, u8) -> bool`
- インスタンス `int->c-long`: `(u8) -> c-long`
- インスタンス `int->c-ulong`: `(u8) -> c-ulong`
- インスタンス `int->char`: `(u8) -> char`
- インスタンス `int->float`: `(u8) -> f64`
- インスタンス `int->i16`: `(u8) -> i16`
- インスタンス `int->i32`: `(u8) -> i32`
- インスタンス `int->i8`: `(u8) -> i8`
- インスタンス `int->int`: `(u8) -> int`
- インスタンス `int->ratio`: `(u8) -> ratio`
- インスタンス `int->u16`: `(u8) -> u16`
- インスタンス `int->u32`: `(u8) -> u32`
- インスタンス `int->u8`: `(u8) -> u8`
- インスタンス `integer-length`: `(u8) -> u8`
- インスタンス `logand`: `(u8, u8) -> u8`
- インスタンス `logbitp`: `(u8, int) -> bool`
- インスタンス `logcount`: `(u8) -> u8`
- インスタンス `logior`: `(u8, u8) -> u8`
- インスタンス `lognot`: `(u8) -> u8`
- インスタンス `logtest`: `(u8, u8) -> bool`
- インスタンス `logxor`: `(u8, u8) -> u8`
- インスタンス `max`: `(u8, u8) -> u8`
- インスタンス `min`: `(u8, u8) -> u8`
- インスタンス `mod`: `(u8, u8) -> u8`
- インスタンス `rem`: `(u8, u8) -> u8`
- インスタンス `try-int->char`: `(u8) -> std::option::Option<char>`
- インスタンス `try-int->i16`: `(u8) -> std::option::Option<i16>`
- インスタンス `try-int->i32`: `(u8) -> std::option::Option<i32>`
- インスタンス `try-int->i8`: `(u8) -> std::option::Option<i8>`
- インスタンス `try-int->u16`: `(u8) -> std::option::Option<u16>`
- インスタンス `try-int->u32`: `(u8) -> std::option::Option<u32>`
- インスタンス `try-int->u8`: `(u8) -> std::option::Option<u8>`

### `std::reader::ReadError` (sum, 型引数 [])

- 変種 0 `ReadError`: `string`

### `std::result::Result` (sum, 型引数 [T, E])

- 変種 0 `Ok`: `T`
- 変種 1 `Err`: `E`

### `std::sexpr::Sexpr` (sum, 型引数 [])

- 変種 0 `Nil`: 
- 変種 1 `Int`: `int`
- 変種 2 `F64`: `f64`
- 変種 3 `Char`: `char`
- 変種 4 `Bool`: `bool`
- 変種 5 `Sym`: `symbol`
- 変種 6 `Str`: `string`
- 変種 7 `Cons`: `std::option::Option<std::sexpr::Sexpr>`, `std::option::Option<std::sexpr::Sexpr>`
- 変種 8 `Bignum`: `int`
- 変種 9 `Ratio`: `ratio`
- 変種 10 `Path`: `std::option::Option<std::sexpr::Sexpr>`
- 変種 11 `F32`: `f32`
- 変種 12 `I8`: `i8`
- 変種 13 `I16`: `i16`
- 変種 14 `U8`: `u8`
- 変種 15 `U16`: `u16`
- 変種 16 `U32`: `u32`
- 変種 17 `I32`: `i32`
- 変種 18 `Vector`: `std::vec::Vector<std::option::Option<std::sexpr::Sexpr>>`
- 変種 19 `Array`: `std::array::Array<std::option::Option<std::sexpr::Sexpr>>`
- 変種 20 `Tuple`: `std::vec::Vector<std::option::Option<std::sexpr::Sexpr>>`
- インスタンス `eq`: `(std::option::Option<std::sexpr::Sexpr>, std::option::Option<std::sexpr::Sexpr>) -> bool`
- インスタンス `eql`: `(std::option::Option<std::sexpr::Sexpr>, std::option::Option<std::sexpr::Sexpr>) -> bool`
- インスタンス `equal`: `(std::sexpr::Sexpr, std::sexpr::Sexpr) -> bool`

### `std::sync::Chan` (struct, 型引数 [T])

- インスタンス `cap`: `(std::sync::Chan<T>) -> int`
- インスタンス `close`: `(std::sync::Chan<T>) -> ()`
- インスタンス `len`: `(std::sync::Chan<T>) -> int`
- 関連関数 `new`: `(int) -> std::sync::Chan<T>`
- インスタンス `recv`: `(std::sync::Chan<T>) -> std::option::Option<T>`
- インスタンス `send`: `(std::sync::Chan<T>, T) -> ()`

### `std::thread::Task` (struct, 型引数 [T])

- インスタンス `wait`: `(std::thread::Task<T>) -> T`

### `std::thread::Thread` (struct, 型引数 [T])

- 関連関数 `available-parallelism`: `() -> int`
- 関連関数 `current-id`: `() -> int`
- インスタンス `join`: `(std::thread::Thread<T>) -> T`

### `std::vec::Vector` (struct, 型引数 [T])

- インスタンス `get`: `(std::vec::Vector<T>, int) -> T`
- インスタンス `len`: `(std::vec::Vector<T>) -> int`
- 関連関数 `new`: `() -> std::vec::Vector<T>`
- インスタンス `pop`: `(std::vec::Vector<T>) -> std::option::Option<T>`
- インスタンス `push`: `(std::vec::Vector<T>, T) -> ()`
- インスタンス `set`: `(std::vec::Vector<T>, int, T) -> ()`

## 組み込み関数

| 関数 | シム | シグネチャ |
|---|---|---|
| `std::%internal::dribble-start` | `rt_dribble_start` | `(string) -> std::result::Result<(),std::fs::FileError>` |
| `std::%internal::dribble-stop` | `rt_dribble_stop` | `() -> std::result::Result<(),std::fs::FileError>` |
| `std::%internal::ed-open` | `rt_ed_open` | `(string, int) -> std::result::Result<(),std::fs::FileError>` |
| `std::%internal::file-create-directories` | `rt_file_create_directories` | `(string) -> std::result::Result<(),std::fs::FileError>` |
| `std::%internal::file-delete` | `rt_file_delete` | `(string) -> std::result::Result<(),std::fs::FileError>` |
| `std::%internal::file-directory-p` | `rt_file_directory_p` | `(string) -> bool` |
| `std::%internal::file-exists-p` | `rt_file_exists_p` | `(string) -> bool` |
| `std::%internal::file-list-directory` | `rt_file_list_directory` | `(string) -> std::result::Result<std::vec::Vector<string>,std::fs::FileError>` |
| `std::%internal::file-modified-date` | `rt_file_modified_date` | `(string) -> std::result::Result<std::time::universal-time,std::fs::FileError>` |
| `std::%internal::file-owner-name` | `rt_file_owner_name` | `(string) -> std::result::Result<std::option::Option<string>,std::fs::FileError>` |
| `std::%internal::file-rename` | `rt_file_rename` | `(string, string) -> std::result::Result<(),std::fs::FileError>` |
| `std::%internal::file-truename` | `rt_file_truename` | `(string) -> std::result::Result<string,std::fs::FileError>` |
| `std::%internal::make-random-state-fresh` | `rt_make_random_state_fresh` | `() -> random-state` |
| `std::%internal::net-accept` | `rt_net_accept` | `(i32) -> std::result::Result<std::option::Option<i32>,std::net::NetError>` |
| `std::%internal::net-buffered-p` | `rt_net_buffered_p` | `(i32) -> std::result::Result<bool,std::net::NetError>` |
| `std::%internal::net-connect-begin` | `rt_net_connect_begin` | `(string) -> std::result::Result<i32,std::net::NetError>` |
| `std::%internal::net-connect-finish` | `rt_net_connect_finish` | `(i32) -> std::result::Result<(),std::net::NetError>` |
| `std::%internal::net-fill` | `rt_net_fill` | `(i32) -> std::result::Result<std::option::Option<int>,std::net::NetError>` |
| `std::%internal::net-flush` | `rt_net_flush` | `(i32) -> std::result::Result<std::option::Option<int>,std::net::NetError>` |
| `std::%internal::net-listen` | `rt_net_listen` | `(string, int) -> std::result::Result<i32,std::net::NetError>` |
| `std::%internal::net-local-address` | `rt_net_local_address` | `(i32) -> std::result::Result<string,std::net::NetError>` |
| `std::%internal::net-peer-address` | `rt_net_peer_address` | `(i32) -> std::result::Result<string,std::net::NetError>` |
| `std::%internal::net-peer-subject` | `rt_net_peer_subject` | `(i32) -> std::result::Result<std::option::Option<string>,std::net::NetError>` |
| `std::%internal::net-pop-byte` | `rt_net_pop_byte` | `(i32) -> std::result::Result<std::option::Option<int>,std::net::NetError>` |
| `std::%internal::net-pop-char` | `rt_net_pop_char` | `(i32) -> std::result::Result<std::option::Option<char>,std::net::NetError>` |
| `std::%internal::net-push-byte` | `rt_net_push_byte` | `(i32, int) -> std::result::Result<(),std::net::NetError>` |
| `std::%internal::net-push-string` | `rt_net_push_string` | `(i32, string) -> std::result::Result<(),std::net::NetError>` |
| `std::%internal::net-resolve-begin` | `rt_net_resolve_begin` | `(string, int) -> std::result::Result<i32,std::net::NetError>` |
| `std::%internal::net-resolve-finish` | `rt_net_resolve_finish` | `(i32) -> std::result::Result<std::option::Option<std::vec::Vector<string>>,std::net::NetError>` |
| `std::%internal::net-server-name` | `rt_net_server_name` | `(i32) -> std::result::Result<std::option::Option<string>,std::net::NetError>` |
| `std::%internal::net-set-keepalive` | `rt_net_set_keepalive` | `(i32, bool) -> std::result::Result<(),std::net::NetError>` |
| `std::%internal::net-set-keepalive-period` | `rt_net_set_keepalive_period` | `(i32, int) -> std::result::Result<(),std::net::NetError>` |
| `std::%internal::net-set-nodelay` | `rt_net_set_nodelay` | `(i32, bool) -> std::result::Result<(),std::net::NetError>` |
| `std::%internal::net-shutdown-write` | `rt_net_shutdown_write` | `(i32) -> std::result::Result<(),std::net::NetError>` |
| `std::%internal::net-socket-error` | `rt_net_socket_error` | `(i32) -> std::result::Result<std::option::Option<string>,std::net::NetError>` |
| `std::%internal::net-tls-add-certificate` | `rt_net_tls_add_certificate` | `(i32, string, string, string) -> std::result::Result<(),std::net::NetError>` |
| `std::%internal::net-tls-handshake` | `rt_net_tls_handshake` | `(i32) -> std::result::Result<std::option::Option<int>,std::net::NetError>` |
| `std::%internal::net-tls-listen` | `rt_net_tls_listen` | `(string, int, string, string, std::option::Option<string>) -> std::result::Result<i32,std::net::NetError>` |
| `std::%internal::net-tls-start` | `rt_net_tls_start` | `(i32, string, std::option::Option<string>, std::option::Option<string>, std::option::Option<string>) -> std::result::Result<(),std::net::NetError>` |
| `std::%internal::net-udp-bind` | `rt_net_udp_bind` | `(string, int) -> std::result::Result<i32,std::net::NetError>` |
| `std::%internal::net-udp-last-sender` | `rt_net_udp_last_sender` | `(i32) -> std::result::Result<string,std::net::NetError>` |
| `std::%internal::net-udp-recv` | `rt_net_udp_recv` | `(i32) -> std::result::Result<std::option::Option<std::vec::Vector<int>>,std::net::NetError>` |
| `std::%internal::net-udp-send-to` | `rt_net_udp_send_to` | `(i32, string, std::vec::Vector<int>) -> std::result::Result<bool,std::net::NetError>` |
| `std::%internal::net-unix-connect` | `rt_net_unix_connect` | `(string) -> std::result::Result<i32,std::net::NetError>` |
| `std::%internal::net-unix-listen` | `rt_net_unix_listen` | `(string) -> std::result::Result<i32,std::net::NetError>` |
| `std::%internal::net-wait` | `rt_suspend_io` | `(i32, int) -> ()` |
| `std::%internal::net-wait-for` | `rt_suspend_io_for` | `(i32, int, f64) -> bool` |
| `std::%internal::random-state-copy` | `rt_random_state_copy` | `(random-state) -> random-state` |
| `std::%internal::random-state-next` | `rt_random_state_next` | `(random-state, int) -> int` |
| `std::%internal::read-datum-at` | `rt_read_datum_at` | `(string, int, bool) -> std::result::Result<(std::option::Option<std::sexpr::Sexpr> . int),std::reader::ReadError>` |
| `std::%internal::stream-at-line-start` | `rt_stream_at_line_start` | `(i32) -> std::result::Result<bool,std::fs::FileError>` |
| `std::%internal::stream-close` | `rt_stream_close` | `(i32) -> std::result::Result<(),std::fs::FileError>` |
| `std::%internal::stream-describe` | `rt_stream_describe` | `(i32) -> string` |
| `std::%internal::stream-finish-output` | `rt_stream_finish_output` | `(i32) -> std::result::Result<(),std::fs::FileError>` |
| `std::%internal::stream-input-p` | `rt_stream_input_p` | `(i32) -> std::result::Result<bool,std::fs::FileError>` |
| `std::%internal::stream-listen` | `rt_stream_listen` | `(i32) -> std::result::Result<bool,std::fs::FileError>` |
| `std::%internal::stream-open-file` | `rt_stream_open_file` | `(string, int) -> std::result::Result<i32,std::fs::FileError>` |
| `std::%internal::stream-open-p` | `rt_stream_open_p` | `(i32) -> bool` |
| `std::%internal::stream-output-p` | `rt_stream_output_p` | `(i32) -> std::result::Result<bool,std::fs::FileError>` |
| `std::%internal::stream-position` | `rt_stream_position` | `(i32) -> std::result::Result<int,std::fs::FileError>` |
| `std::%internal::stream-read-byte` | `rt_stream_read_byte` | `(i32) -> std::result::Result<std::option::Option<int>,std::fs::FileError>` |
| `std::%internal::stream-read-char` | `rt_stream_read_char` | `(i32) -> std::result::Result<std::option::Option<char>,std::fs::FileError>` |
| `std::%internal::stream-stderr` | `rt_stream_stderr` | `() -> i32` |
| `std::%internal::stream-stdin` | `rt_stream_stdin` | `() -> i32` |
| `std::%internal::stream-stdout` | `rt_stream_stdout` | `() -> i32` |
| `std::%internal::stream-string-input` | `rt_stream_string_input` | `(string) -> i32` |
| `std::%internal::stream-string-output` | `rt_stream_string_output` | `() -> i32` |
| `std::%internal::stream-take-output-string` | `rt_stream_take_output_string` | `(i32) -> std::result::Result<string,std::fs::FileError>` |
| `std::%internal::stream-unread-char` | `rt_stream_unread_char` | `(i32, char) -> std::result::Result<(),std::fs::FileError>` |
| `std::%internal::stream-write-byte` | `rt_stream_write_byte` | `(i32, int) -> std::result::Result<(),std::fs::FileError>` |
| `std::%internal::stream-write-string` | `rt_stream_write_string` | `(i32, string) -> std::result::Result<(),std::fs::FileError>` |
| `std::%internal::sxhash-eq` | `rt_sxhash_eq` | `<T> (T) -> int` |
| `std::%internal::sxhash-eql` | `rt_sxhash_eql` | `<T> (T) -> int` |
| `std::%internal::sxhash-equalp` | `rt_sxhash_equalp` | `<T> (T) -> int` |
| `std::board::disable-interrupt` | `rt_interrupt_disable` | `(int) -> ()` |
| `std::board::enable-interrupt` | `rt_interrupt_enable` | `(int) -> ()` |
| `std::cmp::eq` | `rt_sexpr_eq` | `<T> (T, T) -> bool` |
| `std::cmp::eql` | `rt_sexpr_eql` | `<T> (T, T) -> bool` |
| `std::cmp::equalp` | `rt_sexpr_equalp` | `<T> (T, T) -> bool` |
| `std::compile-file` |  | `(string, string) -> bool` |
| `std::dump` |  | `(string) -> bool` |
| `std::env::command-line-args` | `rt_command_line_args` | `() -> std::vec::Vector<string>` |
| `std::env::getenv` | `rt_getenv` | `(string) -> std::option::Option<string>` |
| `std::env::home-directory` | `rt_home_directory` | `() -> std::option::Option<string>` |
| `std::env::lisp-implementation-version` | `rt_lisp_implementation_version` | `() -> string` |
| `std::env::machine-instance` | `rt_machine_instance` | `() -> std::option::Option<string>` |
| `std::env::machine-type` | `rt_machine_type` | `() -> string` |
| `std::env::machine-version` | `rt_machine_version` | `() -> std::option::Option<string>` |
| `std::env::software-type` | `rt_software_type` | `() -> string` |
| `std::env::software-version` | `rt_software_version` | `() -> std::option::Option<string>` |
| `std::eval::eval` | `rt_eval` | `(std::option::Option<std::sexpr::Sexpr>) -> std::result::Result<std::option::Option<std::sexpr::Sexpr>,std::eval::EvalError>` |
| `std::eval::macroexpand` | `rt_macroexpand` | `(std::option::Option<std::sexpr::Sexpr>, &optional, env: std::eval::Environment) -> std::result::Result<std::option::Option<std::sexpr::Sexpr>,std::eval::EvalError>` |
| `std::eval::macroexpand-1` | `rt_macroexpand_1` | `(std::option::Option<std::sexpr::Sexpr>, &optional, env: std::eval::Environment) -> std::result::Result<std::option::Option<std::sexpr::Sexpr>,std::eval::EvalError>` |
| `std::fmt::pprint-indent` | `rt_pprint_indent` | `(symbol, int) -> ()` |
| `std::fmt::pprint-list-exhausted` | `rt_pprint_list_exhausted` | `() -> bool` |
| `std::fmt::pprint-newline` | `rt_pprint_newline` | `(symbol) -> ()` |
| `std::fmt::pprint-pop` | `rt_pprint_pop` | `() -> std::option::Option<std::sexpr::Sexpr>` |
| `std::fmt::pprint-tab` | `rt_pprint_tab` | `(symbol, int, int) -> ()` |
| `std::num::parse-float` | `rt_parse_float` | `(string) -> std::result::Result<f64,std::num::ParseFloatError>` |
| `std::process::exit` | `rt_exit` | `(int) -> !` |
| `std::random::seed-random-state` | `rt_seed_random_state` | `(int) -> random-state` |
| `std::reader::get-dispatch-macro-character` | `rt_get_dispatch_macro_character` | `(char, char) -> std::option::Option<(fn (std::io::string-input-stream char) std::option::Option<std::sexpr::Sexpr>)>` |
| `std::reader::get-macro-character` | `rt_get_macro_character` | `(char) -> std::option::Option<(fn (std::io::string-input-stream char) std::option::Option<std::sexpr::Sexpr>)>` |
| `std::reader::read` | `rt_read` | `(string) -> std::result::Result<std::option::Option<std::sexpr::Sexpr>,std::reader::ReadError>` |
| `std::reader::set-dispatch-macro-character` | `rt_set_dispatch_macro_character` | `(char, char, (fn (std::io::string-input-stream char) std::option::Option<std::sexpr::Sexpr>)) -> ()` |
| `std::reader::set-macro-character` | `rt_set_macro_character` | `(char, (fn (std::io::string-input-stream char) std::option::Option<std::sexpr::Sexpr>)) -> ()` |
| `std::repl::heap-info` | `rt_heap_info` | `() -> std::repl::heap-info` |
| `std::sexpr::sexpr-atom` | `rt_atom` | `(std::option::Option<std::sexpr::Sexpr>) -> bool` |
| `std::sexpr::sexpr-bool` | `rt_sexpr_bool` | `(std::option::Option<std::sexpr::Sexpr>) -> bool` |
| `std::sexpr::sexpr-car` | `rt_car` | `(std::option::Option<std::sexpr::Sexpr>) -> std::option::Option<std::sexpr::Sexpr>` |
| `std::sexpr::sexpr-cdr` | `rt_cdr` | `(std::option::Option<std::sexpr::Sexpr>) -> std::option::Option<std::sexpr::Sexpr>` |
| `std::sexpr::sexpr-char` | `rt_sexpr_char` | `(std::option::Option<std::sexpr::Sexpr>) -> char` |
| `std::sexpr::sexpr-cons` | `rt_cons` | `(std::option::Option<std::sexpr::Sexpr>, std::option::Option<std::sexpr::Sexpr>) -> std::option::Option<std::sexpr::Sexpr>` |
| `std::sexpr::sexpr-consp` | `rt_consp` | `(std::option::Option<std::sexpr::Sexpr>) -> bool` |
| `std::sexpr::sexpr-f32` | `rt_sexpr_f32` | `(std::option::Option<std::sexpr::Sexpr>) -> f32` |
| `std::sexpr::sexpr-f64` | `rt_sexpr_f64` | `(std::option::Option<std::sexpr::Sexpr>) -> f64` |
| `std::sexpr::sexpr-i16` | `rt_sexpr_i16` | `(std::option::Option<std::sexpr::Sexpr>) -> i16` |
| `std::sexpr::sexpr-i32` | `rt_sexpr_i32` | `(std::option::Option<std::sexpr::Sexpr>) -> i32` |
| `std::sexpr::sexpr-i8` | `rt_sexpr_i8` | `(std::option::Option<std::sexpr::Sexpr>) -> i8` |
| `std::sexpr::sexpr-int` | `rt_sexpr_int` | `(std::option::Option<std::sexpr::Sexpr>) -> int` |
| `std::sexpr::sexpr-null` | `rt_null` | `(std::option::Option<std::sexpr::Sexpr>) -> bool` |
| `std::sexpr::sexpr-str` | `rt_sexpr_str` | `(std::option::Option<std::sexpr::Sexpr>) -> string` |
| `std::sexpr::sexpr-sym-name` | `rt_sym_name` | `(std::option::Option<std::sexpr::Sexpr>) -> string` |
| `std::sexpr::sexpr-symp` | `rt_symp` | `(std::option::Option<std::sexpr::Sexpr>) -> bool` |
| `std::sexpr::sexpr-u16` | `rt_sexpr_u16` | `(std::option::Option<std::sexpr::Sexpr>) -> u16` |
| `std::sexpr::sexpr-u32` | `rt_sexpr_u32` | `(std::option::Option<std::sexpr::Sexpr>) -> u32` |
| `std::sexpr::sexpr-u8` | `rt_sexpr_u8` | `(std::option::Option<std::sexpr::Sexpr>) -> u8` |
| `std::symbol::intern` | `rt_intern` | `(string, string) -> symbol` |
| `std::symbol::keywordp` | `rt_keywordp` | `(symbol) -> bool` |
| `std::symbol::symbol->string` | `rt_sym_name` | `(symbol) -> string` |
| `std::thread::sleep` | `rt_suspend_sleep` | `(f64) -> ()` |
| `std::thread::yield` | `rt_suspend_yield` | `() -> ()` |
| `std::time::get-internal-real-time` | `rt_get_internal_real_time` | `() -> std::time::internal-time` |
| `std::time::get-internal-run-time` | `rt_get_internal_run_time` | `() -> std::time::internal-time` |
| `std::time::get-universal-time` | `rt_get_universal_time` | `() -> std::time::universal-time` |
| `std::time::timezone-daylight-p` | `rt_timezone_daylight_p` | `(int, int) -> std::option::Option<bool>` |
| `std::time::timezone-offset-seconds` | `rt_timezone_offset_seconds` | `(int, int) -> std::option::Option<int>` |

