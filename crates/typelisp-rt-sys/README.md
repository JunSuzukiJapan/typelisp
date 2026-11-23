# typelisp-rt-sys

An internal crate of [typelisp](https://github.com/JunSuzukiJapan/typelisp): the part of the runtime that compiled programs link which needs files and a process environment.
It is published only because the `typelisp` crate depends on it, and its API
is not meant to be used directly or kept stable.

[typelisp](https://github.com/JunSuzukiJapan/typelisp) の内部クレートです（コンパイルしたプログラムがリンクするランタイムのうち、ファイルとプロセスの環境が要る部分）。
`typelisp` クレートが依存しているので公開していますが、直接使うことは想定しておらず、
API の互換性も保ちません。

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your
option, with the typelisp Runtime Exception (LICENSE-EXCEPTION).
